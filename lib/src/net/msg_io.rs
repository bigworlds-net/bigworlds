use std::net::SocketAddr;

use message_io::node::NodeHandler;
use tokio_stream::StreamExt;

use engine_core::common::net::Encoding;
use engine_core::common::util::Shutdown;
use engine_core::executor::Executor;

use crate::util_net::{decode, encode};
use crate::Result;

pub struct MioHandle<REQ, RESP> {
    pub exec: Executor<REQ, Result<RESP>>,
}

pub fn spawn<
    REQ: Clone + Send + 'static + serde::de::DeserializeOwned,
    RESP: Clone + Send + 'static + serde::Serialize,
>(
    tcp: Option<SocketAddr>,
    udp: Option<SocketAddr>,
    ws: Option<SocketAddr>,
    net_exec: Executor<(SocketAddr, REQ), Result<RESP>>,
    mut shutdown: Shutdown,
) -> Result<()> {
    let (exec_sender, exec_receiver) =
        tokio::sync::broadcast::channel::<(message_io::network::Endpoint, Vec<u8>)>(20);
    // Make the network channel receiver into a stream
    let mut exec_stream = tokio_stream::wrappers::BroadcastStream::new(exec_receiver);

    // Create a node, the main message-io entity. It is divided in 2 parts:
    // The 'handler', used to make actions (connect, send messages, signals, stop the node...)
    // The 'listener', used to read events from the network or signals.
    let (net_handler, net_listener) = message_io::node::split::<()>();

    // Message-io based networking is sufficient for most cases.
    // It enables listening on FramedTcp, Udp and WebSocket transports.
    use message_io::network::{NetEvent, Transport};
    use message_io::node::{split, NodeEvent};

    let net_handler_c = net_handler.clone();
    let mut shutdownc = shutdown.clone();
    tokio::spawn(async move {
        shutdownc.recv().await;
        net_handler_c.signals().send(());
    });

    let net_handler_c = net_handler.clone();
    tokio::spawn(async move {
        if let Some(addr) = tcp {
            net_handler_c
                .network()
                .listen(Transport::FramedTcp, addr)
                .unwrap();
        }
        if let Some(addr) = udp {
            net_handler_c
                .network()
                .listen(Transport::Udp, addr)
                .unwrap();
        }
        if let Some(addr) = ws {
            net_handler_c.network().listen(Transport::Ws, addr).unwrap();
        }

        // Read incoming network events.
        net_listener.for_each_async(move |event| match event {
            NodeEvent::Network(ne) => match ne {
                NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
                NetEvent::Accepted(_endpoint, _listener) => println!("Client connected"), // Tcp or Ws
                NetEvent::Message(endpoint, data) => {
                    println!(
                        "Received message: endpoint: {:?}, data: {:?}",
                        endpoint, data
                    );
                    exec_sender
                        .send((endpoint, data.to_owned()))
                        .expect("failed sending network message to channel");
                }
                NetEvent::Disconnected(_endpoint) => println!("Client disconnected"), //Tcp or Ws
            },
            NodeEvent::Signal(_) => {
                net_handler_c.stop();
            }
        });
    });

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(Ok((endpoint, bytes))) = exec_stream.next() => {
                    // let net_exec_cc = net_exec_c.clone();
                    let net_exec_c = net_exec.clone();
                    let net_handler_c = net_handler.clone();
                    // decode
                    let req: REQ = decode(&bytes, &Encoding::Bincode).unwrap();
                    tokio::spawn(async move {
                        println!("msg_io: new task");
                        let resp: RESP = net_exec_c.execute((endpoint.addr(), req)).await.unwrap().unwrap();
                        net_handler_c.network().send(endpoint, &encode(resp, &Encoding::Bincode).unwrap());
                        println!("msg_io: new task sent back");
                    });
                }
                _ = shutdown.recv() => break,
            }
        }
    });
    Ok(())
}
