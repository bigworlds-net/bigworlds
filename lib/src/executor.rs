use std::marker::PhantomData;

use byteorder::WriteBytesExt;
use integer_encoding::{FixedInt, VarIntReader, VarIntWriter};
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

use uuid::Uuid;

use crate::rpc::Participant;
use crate::{rpc, Error, Result};

#[async_trait::async_trait]
pub trait Executor<IN, OUT> {
    async fn execute(&self, req: IN) -> Result<OUT>;
}

#[async_trait::async_trait]
pub trait ExecutorMulti<IN, OUT> {
    async fn execute_to_multi(&self, req: IN) -> Result<Receiver<OUT>>;
}

/// Local executor supporting both regular request/response and
/// request/multi-response patterns.
#[derive(Clone)]
pub struct LocalExec<IN, OUT> {
    sender: mpsc::Sender<(IN, oneshot::Sender<OUT>)>,
    sender_multi: mpsc::Sender<(IN, mpsc::Sender<OUT>)>,
}

impl<IN, OUT> LocalExec<IN, OUT> {
    pub fn new(
        capacity: usize,
    ) -> (
        Self,
        tokio_stream::wrappers::ReceiverStream<(IN, oneshot::Sender<OUT>)>,
        tokio_stream::wrappers::ReceiverStream<(IN, mpsc::Sender<OUT>)>,
    ) {
        let (mut sender, receiver) =
            tokio::sync::mpsc::channel::<(IN, oneshot::Sender<OUT>)>(capacity);
        let mut stream = tokio_stream::wrappers::ReceiverStream::new(receiver);

        let (mut sender_multi, receiver_multi) =
            tokio::sync::mpsc::channel::<(IN, mpsc::Sender<OUT>)>(capacity);
        let mut stream_multi = tokio_stream::wrappers::ReceiverStream::new(receiver_multi);
        (
            Self::new_from_senders(sender, sender_multi),
            stream,
            stream_multi,
        )
    }

    pub fn new_from_senders(
        sender: mpsc::Sender<(IN, oneshot::Sender<OUT>)>,
        sender_multi: mpsc::Sender<(IN, mpsc::Sender<OUT>)>,
    ) -> Self {
        Self {
            sender,
            sender_multi,
        }
    }
}

#[async_trait::async_trait]
impl<IN: Send, OUT: Send> Executor<IN, OUT> for LocalExec<IN, OUT> {
    async fn execute(&self, msg: IN) -> Result<OUT> {
        let (sender, receiver) = oneshot::channel::<OUT>();
        self.sender.send((msg, sender)).await.map_err(|e| {
            Error::Other(format!(
                "local executor failed sending, receiver dropped: {e}"
            ))
        })?;
        Ok(receiver.await?)
    }
}

#[async_trait::async_trait]
impl<IN: Send, OUT: Send> ExecutorMulti<IN, OUT> for LocalExec<IN, OUT> {
    async fn execute_to_multi(&self, msg: IN) -> Result<Receiver<OUT>> {
        let (sender, receiver) = mpsc::channel::<OUT>(1);
        self.sender_multi.send((msg, sender)).await.map_err(|e| {
            Error::Other(format!(
                "local executor failed sending, receiver dropped: {e}"
            ))
        })?;
        Ok(Receiver::Tokio(receiver))
    }
}

#[derive(Clone)]
pub struct RemoteExec<IN, OUT> {
    connection: quinn::Connection,
    phantom: PhantomData<(IN, OUT)>,
}

impl<IN, OUT> RemoteExec<IN, OUT> {
    pub fn new(connection: quinn::Connection) -> Self {
        RemoteExec {
            connection,
            phantom: Default::default(),
        }
    }

    pub fn remote_address(&self) -> std::net::SocketAddr {
        self.connection.remote_address()
    }

    pub fn close_reason(&self) -> Option<String> {
        self.connection
            .close_reason()
            .map(|reason| reason.to_string())
    }
}

#[async_trait::async_trait]
impl<IN: Send + Sync + serde::Serialize, OUT: Send + Sync + serde::de::DeserializeOwned>
    Executor<IN, OUT> for RemoteExec<IN, OUT>
{
    async fn execute(&self, msg: IN) -> Result<OUT> {
        let out_bytes = {
            let (mut send, recv) = self
                .connection
                .open_bi()
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            trace!("got both ends of stream");
            let msg = bincode::serialize(&msg).map_err(|e| Error::Other(e.to_string()))?;
            trace!("serialized message");

            // Signify this is a regular rpc call.
            send.write_u8(0).await?;

            send.write_all(&msg)
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            send.finish()
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            trace!("wrote all msg");
            let out_bytes = recv
                .read_to_end(100000000)
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
            if out_bytes.len() < 10000 {
                trace!("outbytes: {:?}", out_bytes);
            }
            out_bytes
        };

        let out: OUT = bincode::deserialize(&out_bytes).map_err(|e| Error::Other(e.to_string()))?;
        Ok(out)
    }
}

#[async_trait::async_trait]
impl<IN: Send + Sync + serde::Serialize, OUT: Send + Sync + serde::de::DeserializeOwned>
    ExecutorMulti<IN, OUT> for RemoteExec<IN, OUT>
{
    async fn execute_to_multi(&self, msg: IN) -> Result<Receiver<OUT>> {
        let (mut send, recv) = self
            .connection
            .open_bi()
            .await
            .map_err(|e| Error::QuinnNetworkError(e.to_string()))?;

        let msg = bincode::serialize(&msg)?;

        // Signify this is a streaming response call.
        send.write_u8(1).await?;
        // Write the message into the buffer.
        send.write_all(&msg)
            .await
            .map_err(|e| Error::QuinnNetworkError(e.to_string()))?;
        send.finish()
            .await
            .map_err(|e| Error::QuinnNetworkError(e.to_string()))?;

        Ok(QuinnReceiver {
            recv,
            _marker: PhantomData,
        }
        .into())
    }
}

pub enum Receiver<T> {
    Tokio(tokio::sync::mpsc::Receiver<T>),
    Quinn(QuinnReceiver<T>),
}

impl<T: serde::de::DeserializeOwned> Receiver<T> {
    pub async fn recv(&mut self) -> Result<Option<T>> {
        match self {
            Self::Tokio(rx) => Ok(rx.recv().await),
            Self::Quinn(rx) => Ok(rx.recv().await?),
        }
    }
}

impl<T: serde::de::DeserializeOwned> From<QuinnReceiver<T>> for Receiver<T> {
    fn from(recv: QuinnReceiver<T>) -> Self {
        Receiver::Quinn(recv)
    }
}

pub struct QuinnReceiver<T> {
    recv: quinn::RecvStream,
    _marker: std::marker::PhantomData<T>,
}

/// Default max message size (16 MiB).
pub const MAX_MESSAGE_SIZE: u64 = 1024 * 1024 * 16;

impl<T: serde::de::DeserializeOwned> QuinnReceiver<T> {
    async fn recv(&mut self) -> Result<Option<T>> {
        let read = &mut self.recv;

        // TODO: handle unhappy cases, potentially returning Ok(None), e.g.
        // when there's not enough data to read the length marker.
        let size = read.read_u64().await?;

        if size > MAX_MESSAGE_SIZE {
            self.recv.stop((1 as u32).into()).ok();
            return Err(Error::QuinnNetworkError(
                "max message size exceeded".to_owned(),
            ));
        }

        let mut buf = vec![0; size as usize];
        read.read_exact(&mut buf)
            .await
            .map_err(|e| Error::QuinnNetworkError(e.to_string()));
        let msg: T = bincode::deserialize(&buf)?;

        Ok(Some(msg))
    }
}

impl<T> Drop for QuinnReceiver<T> {
    fn drop(&mut self) {}
}
