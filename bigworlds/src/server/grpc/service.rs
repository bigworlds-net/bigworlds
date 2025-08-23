pub mod bigworlds {
    tonic::include_proto!("bigworlds"); // The string specified here must match the proto package name
}

pub use bigworlds::worlds_server::WorldsServer as Server;
use tonic::IntoRequest;

use crate::{
    executor::{Executor, ExecutorMulti, LocalExec},
    rpc::msg::Message,
    Query,
};

use super::super::ConnectionOrAddress;

pub struct Service {
    pub executor: LocalExec<(ConnectionOrAddress, Message), Message>,
}

#[tonic::async_trait]
impl bigworlds::worlds_server::Worlds for Service {
    async fn register_client(
        &self,
        request: tonic::Request<bigworlds::RegisterClientRequest>,
    ) -> Result<tonic::Response<bigworlds::RegisterClientResponse>, tonic::Status> {
        let addr = request.remote_addr().expect("remote address not available");
        let resp = self
            .executor
            .execute((
                ConnectionOrAddress::Address(addr),
                request.into_inner().into(),
            ))
            .await
            .unwrap();
        Ok(tonic::Response::new(resp.into()))
    }

    type SubscribeStream =
        tokio_stream::wrappers::ReceiverStream<Result<bigworlds::SubscribeResponse, tonic::Status>>;
    async fn subscribe(
        &self,
        request: tonic::Request<bigworlds::SubscribeRequest>,
    ) -> Result<tonic::Response<Self::SubscribeStream>, tonic::Status> {
        let addr = request.remote_addr().expect("remote address not available");

        let (tx, rx) = tokio::sync::mpsc::channel(1);

        let mut receiver = self
            .executor
            .execute_to_multi((
                ConnectionOrAddress::Address(addr),
                request.into_inner().into(),
            ))
            .await
            .unwrap();

        tokio::spawn(async move {
            loop {
                let res = receiver.recv().await;
                match res {
                    Ok(Some(msg)) => {
                        tx.send(Ok(msg.into())).await.unwrap();
                        continue;
                    }
                    Err(e) => {
                        tx.send(Err(tonic::Status::aborted(e.to_string())))
                            .await
                            .unwrap();
                        break;
                    }
                    _ => break,
                }
            }
        });

        Ok(tonic::Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn query(
        &self,
        request: tonic::Request<bigworlds::QueryRequest>,
    ) -> Result<tonic::Response<bigworlds::QueryResponse>, tonic::Status> {
        let addr = request.remote_addr().expect("remote address not available");
        let resp = self
            .executor
            .execute((
                ConnectionOrAddress::Address(addr),
                request.into_inner().into(),
            ))
            .await
            .unwrap();
        Ok(tonic::Response::new(resp.into()))
    }
}
