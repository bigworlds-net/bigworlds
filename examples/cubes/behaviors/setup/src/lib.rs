#![allow(warnings)]

use futures::future::BoxFuture;

use bigworlds::{
    Signal,
    Address, Executor, LocalExec, Query, QueryProduct, Result, Var, query,
    rpc::{
        self,
        behavior::{Request, Response},
    },
};
use tokio_stream::StreamExt;

#[unsafe(no_mangle)]
pub fn main(
    // Events happening on the worker.
    mut events: tokio_stream::wrappers::ReceiverStream<(
        Signal<rpc::behavior::Request>,
        tokio::sync::oneshot::Sender<Result<Signal<rpc::behavior::Response>>>,
    )>,
    // Worker executor handle.
    worker: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
) -> BoxFuture<'static, Result<()>> {
    Box::pin(async move {
        for n in 0..1000 {
            // Spawn a cube from prefab.
            let _ = worker
                .execute(Signal::from(rpc::worker::Request::SpawnEntity {
                    name: format!("cube_{n}"),
                    prefab: Some("cube".to_owned()),
                }))
                .await??;

            // Modify position such that each cube is 1 unit higher than the
            // previous one.
            let addr = format!("cube_{}.position.float.y", n).parse()?;
            worker
                .execute(Signal::from(rpc::worker::Request::SetVar(addr, Var::Float(n as f32))))
                .await??;
        }

        // worker
        //     .execute(rpc::worker::Request::RegisterQuery(
        //         query::Trigger::StepEvent,
        //         Query {
        //             ..Default::default()
        //         },
        //     ))
        //     .await??;

        loop {
            match events.next().await {
                Some((sig, snd)) => {
                    match sig.payload {
                        Request::Event(event) => match event.as_str() {
                            "step" => {
                                // println!("behavior: event `never`");
                                // let product: QueryProduct = worker
                                //     .execute(rpc::worker::Request::ProcessQuery {
                                //         query: Query {
                                //             description: query::Description::Addressed,
                                //             layout: query::Layout::Var,
                                //             filters: vec![query::Filter::Component(
                                //                 "spatial".to_owned(),
                                //             )],
                                //             mappings: vec![query::Map::Components(vec![
                                //                 "spatial".to_owned(),
                                //             ])],
                                //             scope: query::Scope::Local,
                                //             ..Default::default()
                                //         },
                                //         went_through_leader: false,
                                //         went_through_workers: vec![],
                                //     })
                                //     .await??
                                //     .try_into()?;

                                // // println!("{:?}", product);

                                // let now = std::time::Instant::now();
                                // if let QueryProduct::AddressedVar(vars) = product {
                                //     for (addr, mut var) in vars {
                                //         if addr.var_name == "position" {
                                //             if let Var::Vec2(x, y) = &mut var {
                                //                 *x += 0.01;
                                //                 *y += 0.033;
                                //             }
                                //         }

                                //         // if addr.entity.contains("11") {
                                //         worker
                                //             .execute(rpc::worker::Request::SetVar(addr, var))
                                //             .await??;
                                //         // }
                                //     }
                                // } else {
                                //     println!("unimplemented")
                                // }
                                // println!(
                                //     "behavior mutations time: {}ms",
                                //     now.elapsed().as_millis()
                                // );

                                snd.send(Ok(Signal::new(Response::Empty, sig.ctx)));
                                // println!("[behavior::global] finished processing");
                            }
                            _ => {
                                snd.send(Ok(Signal::new(Response::Empty, sig.ctx)));
                            }
                        },
                        Request::Shutdown => {
                            snd.send(Ok(Signal::new(Response::Empty, sig.ctx)));
                            break;
                        }
                    }
                }
                None => (),
            }
        }

        Ok(())
    })
}
