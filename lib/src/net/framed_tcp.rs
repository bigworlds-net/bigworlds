use std::net::SocketAddr;

use bytes::{Buf, BytesMut};
use integer_encoding::VarInt;
use serde::Deserialize;

use futures::sink::SinkExt;
use futures::{StreamExt, TryStreamExt};
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::{Decoder, Encoder, Framed};
use tokio_util::sync::{CancellationToken, PollSender};

use crate::executor::{Executor, LocalExec};
use crate::net::Encoding;
use crate::util_net::{decode, encode};
use crate::{Error, Result};

const MAX: usize = 8 * 1024 * 1024;

// pub async fn spawn_connection(
//     endpoint: SocketAddr,
//     mut send_stream: ReceiverStream<(Vec<u8>, tokio::sync::oneshot::Sender<Result<Vec<u8>>>)>,
//     runtime: runtime::Handle,
//     mut cancel: CancellationToken,
// ) -> Result<()> {
//     let stream = tokio::net::TcpStream::connect(endpoint).await?;
//
//     runtime.clone().spawn(async move {
//         // frame the stream into message chunks using the framed codec
//         let mut framed = Framed::new(stream, FramedTcpCodec {});
//         let (mut write, mut read) = framed.split();
//
//         // use a channel to write messages to stream in a separate thread
//         let (snd_write, mut rcv_write) = tokio::sync::mpsc::channel::<Vec<u8>>(20);
//         let mut rcv_write_stream =
//             tokio_stream::wrappers::ReceiverStream::new(rcv_write).map(|b| Ok(b));
//         runtime.clone().spawn(async move {
//             write.send_all(&mut rcv_write_stream).await;
//         });
//
//         // use a channel to read messages from stream in a separate thread
//         let (mut snd_read, mut rcv_read) = tokio::sync::mpsc::channel::<Vec<u8>>(20);
//         let mut rcv_read_stream = tokio_stream::wrappers::ReceiverStream::new(rcv_read);
//         runtime.clone().spawn(async move {
//             while let Some(item) = read.next().await {
//                 if snd_read.send(item.unwrap()).await.is_err() {
//                     break;
//                 }
//             }
//         });
//
//         runtime.clone().spawn(async move {
//             loop {
//                 let snd_write_c = snd_write.clone();
//                 tokio::select! {
//                     // process a read from send stream
//                     Some((msg, sender)) = send_stream.next() => {
//                         runtime.clone().spawn(async move {
//                             println!("sending msg {:?}", msg);
//                             snd_write_c.clone().send(msg);
//
//                             // tokio::time::sleep(Duration::from_secs(1)).await;
//                             if let Some(bytes) = rcv_read.recv().await {
//                                 println!("got response: {:?}", bytes);
//                                 sender.send(Ok(bytes));
//                             }
//                         });
//                     },
//                     // process a successful read from endpoint
//                     // Some(Ok(bytes)) = read.next() => {
//                         // let addr_c = addr.clone();
//
//                         // asynchronously process the incoming message
//                         // runtime_cc.spawn(async move {
//                         //     // send the incoming message to the executor,
//                         //     // wait until it comes back with a response
//                         //     let resp_bytes = match exec_cc.execute((addr_c, bytes)).await {
//                         //         Ok(r) => r,
//                         //         Err(e) => {
//                         //             warn!("executor failed: {:?}", e);
//                         //             return;
//                         //         }
//                         //     };
//                         //     snd_c.send(resp_bytes).await;
//                         // });
//                     // }
//                     _ = cancel.recv() => break,
//                 }
//             }
//         });
//     });
//
//     Ok(())
// }

fn spawn_stream_handler(addr: SocketAddr) {
    //
}

pub fn spawn_listener(
    address: SocketAddr,
    exec: LocalExec<(super::ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    mut cancel: CancellationToken,
) {
    tokio::spawn(async move {
        // listen to incoming connections at the provided address
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        loop {
            if cancel.is_cancelled() {
                break;
            }

            // accept incoming connection as a stream
            let (mut stream, addr) = listener.accept().await.unwrap();
            // frame the stream into message chunks using the framed codec
            let mut framed = Framed::new(stream, FramedTcpCodec {});
            let (mut write, mut read) = framed.split();

            // use a channel to write messages to stream in a separate thread
            let (snd, mut rcv) = tokio::sync::mpsc::channel::<Vec<u8>>(20);
            let mut rcv_stream = tokio_stream::wrappers::ReceiverStream::new(rcv).map(|b| Ok(b));
            tokio::spawn(async move {
                write.send_all(&mut rcv_stream).await;
            });

            let exec_c = exec.clone();
            let mut cancel_c = cancel.clone();
            tokio::spawn(async move {
                loop {
                    let exec_cc = exec_c.clone();
                    let snd_c = snd.clone();

                    // either process a successful read or shut down
                    tokio::select! {
                        Some(Ok(bytes)) = read.next() => {
                            let addr_c = addr.clone();

                            // asynchronously process the incoming message
                            tokio::spawn(async move {
                                // send the incoming message to the executor,
                                // wait until it comes back with a response
                                let resp_bytes = match exec_cc.clone().execute((super::ConnectionOrAddress::Address(addr_c), bytes)).await {
                                    Ok(r) => r,
                                    Err(e) => {
                                        warn!("executor failed: {:?}", e);
                                        return;
                                    }
                                };
                                snd_c.send(resp_bytes).await;
                            });
                        }
                        _ = cancel_c.cancelled() => {
                            break;
                        }
                    }
                }
            });
        }
    });
}

const MAX_SIZE_LEN: usize = 10;

/// Decodes an encoded value in a buffer.
/// The function returns the message size and the consumed bytes or none if the buffer is too small.
pub fn decode_size(data: &[u8]) -> Option<(usize, usize)> {
    usize::decode_var(data)
}

/// Encode a message, returning the bytes that must be sent before the message.
/// A buffer is used to avoid heap allocation.
pub fn encode_size<'a>(message: &[u8], buf: &'a mut [u8; MAX_SIZE_LEN]) -> &'a [u8] {
    let varint_size = message.len().encode_var(buf);
    &buf[..varint_size]
}

pub struct FramedTcpCodec;

impl Decoder for FramedTcpCodec {
    type Item = Vec<u8>;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let (length, consumed_count) = match decode_size(src) {
            Some(l) => l,
            None => return Ok(None), // Not enough data to read length marker.
        };

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if length > MAX {
            return Err(Error::InvalidData(format!(
                "Frame of length {} is too large.",
                length
            )));
        }

        if src.len() < consumed_count + length {
            // The full string has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(consumed_count + length - src.len());

            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }

        // Use advance to modify src such that it no longer contains
        // this frame.
        let data = src[consumed_count..consumed_count + length].to_vec();
        src.advance(consumed_count + length);

        Ok(Some(data))
    }
}

impl Encoder<Vec<u8>> for FramedTcpCodec {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        item: Vec<u8>,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        let mut buf = [0; MAX_SIZE_LEN];
        dst.extend_from_slice(&*encode_size(&item, &mut buf));
        dst.extend_from_slice(&item);
        Ok(())
    }
}
