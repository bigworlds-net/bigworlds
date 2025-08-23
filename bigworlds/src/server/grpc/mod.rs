pub mod conversions;
pub mod query_conversions;
pub mod service;

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::rpc::msg::Message;
use crate::server::ConnectionOrAddress;
use tokio_util::sync::CancellationToken;

pub fn spawn(
    listener_addr: CompositeAddress,
    executor: LocalExec<(ConnectionOrAddress, Message), Message>,
    cancel: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sock_addr = if let crate::net::Address::Net(addr) = listener_addr.address {
        addr
    } else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unable to use provided address to spawn a gRPC server",
        )));
    };

    let service = service::Service { executor };

    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(service::bigworlds::worlds_server::WorldsServer::new(
                service,
            ))
            .serve_with_shutdown(sock_addr, async {
                cancel.cancelled().await;
            })
            .await;
    });

    Ok(())
}
