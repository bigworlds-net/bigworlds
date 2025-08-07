mod conversions;
mod service;

use tokio_util::sync::CancellationToken;

use crate::{
    net::{CompositeAddress, ConnectionOrAddress},
    rpc::msg::Message,
    Error, LocalExec, Result,
};

pub struct GrpcServerHandler {}

pub fn spawn(
    listener_addr: CompositeAddress,
    exec: LocalExec<(ConnectionOrAddress, Message), Message>,
    cancel: CancellationToken,
) -> Result<GrpcServerHandler> {
    println!("spawning grpc server");

    let sock_addr = if let crate::net::Address::Net(addr) = listener_addr.address {
        addr
    } else {
        return Err(Error::NetworkError(format!(
            "unable to use provided address to spawn a grpc adapter on server: {}",
            listener_addr
        )));
    };

    let _ = tokio::spawn(async move {
        let service = service::Service { executor: exec };

        tokio::select! {
            result = tonic::transport::Server::builder()
                        .add_service(service::Server::new(service))
                        .serve(sock_addr)
                    => result.expect("grpc server failed"),
            _ = cancel.cancelled() => (),
        };
    });

    Ok(GrpcServerHandler {})
}
