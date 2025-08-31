use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use crate::executor::{Executor, LocalExec};
use crate::{Error, Result};

use super::ConnectionOrAddress;

pub fn spawn_listener(
    address: SocketAddr,
    exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    mut cancel: CancellationToken,
) {
    tokio::spawn(async move {
        let listener = TcpListener::bind(address).await.unwrap();
        loop {
            let (mut stream, addr) = listener.accept().await.unwrap();
            let mut ws = match tokio_tungstenite::accept_async(stream).await {
                Ok(w) => w,
                Err(e) => {
                    error!("websocket handshake failed: {:?}", e);
                    continue;
                }
            };
            let (mut write, mut read) = ws.split();

            let (snd, mut rcv) = tokio::sync::mpsc::channel::<Vec<u8>>(20);
            let mut rcv_stream = tokio_stream::wrappers::ReceiverStream::new(rcv)
                .map(|b| Ok(tokio_tungstenite::tungstenite::Message::Binary(b.into())));
            tokio::spawn(async move {
                write.send_all(&mut rcv_stream).await;
            });

            let exec_c = exec.clone();
            let mut cancel = cancel.clone();
            tokio::spawn(async move {
                loop {
                    // println!("reading");
                    let mut exec_cc = exec_c.clone();
                    let snd_c = snd.clone();
                    tokio::select! {
                        Some(Ok(msg)) = read.next() => {
                            let addr_c = addr.clone();
                            tokio::spawn(async move {
                                let msg: Message = msg;
                                let bytes = match msg {
                                    Message::Binary(b) => b,
                                    Message::Close(c) => {
                                        if let Some(c_) = c {
                                            warn!("close frame: {:?}", c_);
                                        }
                                        return;
                                    },
                                    _ => unimplemented!("{:?}", msg),
                                };

                                match exec_cc.execute((ConnectionOrAddress::Address(addr_c), bytes.to_vec())).await {
                                    Ok(resp_bytes) => if let Err(e) = snd_c.send(resp_bytes).await {
                                        error!("{}", e.to_string());
                                    },
                                    Err(e) => error!("{}", e.to_string()),
                                };

                            });
                        }
                        _ = cancel.cancelled() => break,
                    }
                }
            });
        }
    });
}
