pub mod protoworlds {
    tonic::include_proto!("protoworlds"); // The string specified here must match the proto package name
}

pub use protoworlds::worlds_server::WorldsServer as Server;
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
impl protoworlds::worlds_server::Worlds for Service {
    async fn register_client(
        &self,
        request: tonic::Request<protoworlds::RegisterClientRequest>,
    ) -> Result<tonic::Response<protoworlds::RegisterClientResponse>, tonic::Status> {
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

    type SubscribeStream = tokio_stream::wrappers::ReceiverStream<
        Result<protoworlds::SubscribeResponse, tonic::Status>,
    >;
    async fn subscribe(
        &self,
        request: tonic::Request<protoworlds::SubscribeRequest>,
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
        _request: tonic::Request<protoworlds::QueryRequest>,
    ) -> Result<tonic::Response<protoworlds::QueryResponse>, tonic::Status> {
        Ok(tonic::Response::new(protoworlds::QueryResponse {
            result: String::new(),
        }))
    }
}
