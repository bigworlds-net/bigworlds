use futures::future::BoxFuture;

use bigworlds::{
    query,
    rpc::{
        self,
        behavior::{Request, Response},
    },
    Executor, LocalExec, Query, QueryProduct, Result, Var,
};
use tokio_stream::StreamExt;
// use uuid::Uuid;

#[no_mangle]
pub fn global_synced(
    // events happening on the worker
    mut events: tokio_stream::wrappers::ReceiverStream<(
        rpc::behavior::Request,
        tokio::sync::oneshot::Sender<Result<rpc::behavior::Response>>,
    )>,
    // worker executor handle
    worker: LocalExec<rpc::worker::Request, Result<rpc::worker::Response>>,
) -> BoxFuture<'static, Result<()>> {
    Box::pin(async move {
        // for n in 0..10000 {
        //     let _ = worker
        //         .execute(rpc::worker::Request::SpawnEntity {
        //             name: format!("boid_{n}"),
        //             prefab: Some("boid".to_owned()),
        //         })
        //         .await??;
        // }

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
                Some((req, snd)) => {
                    match req {
                        Request::Subscription(name, product) => {
                            println!("got query subscription product");
                            snd.send(Ok(Response::Empty));
                        }
                        Request::Event(event) => match event.as_str() {
                            "step" => {
                                println!("behavior: event `never`");
                                let product: QueryProduct = worker
                                    .execute(rpc::worker::Request::ProcessQuery {
                                        query: Query {
                                            description: query::Description::Addressed,
                                            layout: query::Layout::Var,
                                            filters: vec![query::Filter::Component(
                                                "spatial".to_owned(),
                                            )],
                                            mappings: vec![query::Map::Components(vec![
                                                "spatial".to_owned()
                                            ])],
                                            scope: query::Scope::Local,
                                            ..Default::default()
                                        },
                                        went_through_leader: false,
                                        went_through_workers: vec![],
                                    })
                                    .await??
                                    .try_into()?;

                                // println!("{:?}", product);

                                let now = std::time::Instant::now();
                                if let QueryProduct::AddressedVar(vars) = product {
                                    for (addr, mut var) in vars {
                                        if addr.var_name == "position" {
                                            if let Var::Vec2(x, y) = &mut var {
                                                *x += 0.01;
                                                *y += 0.033;
                                            }
                                        }

                                        // if addr.entity.contains("11") {
                                            worker
                                                .execute(rpc::worker::Request::SetVar(addr, var))
                                                .await??;
                                        // }
                                    }
                                } else {
                                    unimplemented!()
                                }
                                println!(
                                    "behavior mutations time: {}ms",
                                    now.elapsed().as_millis()
                                );

                                snd.send(Ok(Response::Empty));
                                // println!("[behavior::global] finished processing");
                            }
                            _ => {
                                snd.send(Ok(Response::Empty));
                            }
                        },
                        Request::Shutdown => {
                            snd.send(Ok(Response::Empty));
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

/// Computes the flocking behavior for all the boids.
#[no_mangle]
pub fn global(
    // events happening on the worker
    mut events: tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    // worker executor handle
    worker: LocalExec<rpc::worker::Request, Result<rpc::worker::Response>>,
) -> BoxFuture<'static, Result<()>> {
    Box::pin(async move {
        // for n in 0..1000 {
        //     let _ = worker
        //         .execute(rpc::worker::Request::SpawnEntity {
        //             name: format!("boid_{n}"),
        //             prefab: Some("boid".to_owned()),
        //         })
        //         .await??;
        // }

        loop {
            match events.recv().await {
                Ok(req) => match req {
                    Request::Event(event) => match event.as_str() {
                        "step" => {
                            process_step();
                            // println!("[boids::behavior::global] attempting a query");
                            let product: QueryProduct = worker
                                .execute(rpc::worker::Request::ProcessQuery {
                                    query: Query {
                                        description: query::Description::Addressed,
                                        layout: query::Layout::Var,
                                        filters: vec![query::Filter::Component(
                                            "spatial".to_owned(),
                                        )],
                                        mappings: vec![query::Map::Components(vec![
                                            "spatial".to_owned()
                                        ])],
                                        scope: query::Scope::Local,
                                        ..Default::default()
                                    },
                                    went_through_leader: false,
                                    went_through_workers: vec![],
                                })
                                .await??
                                .try_into()?;

                            // println!("{:?}", product);

                            if let QueryProduct::AddressedVar(vars) = product {
                                for (addr, mut var) in vars {
                                    if addr.var_name == "position" {
                                        if let Var::Vec2(x, y) = &mut var {
                                            *x += 0.01;
                                            *y += 0.033;
                                        }
                                    }

                                    worker
                                        .execute(rpc::worker::Request::SetVar(addr, var))
                                        .await??;
                                }
                            } else {
                                unimplemented!()
                            }

                            println!("[behavior::global] finished processing");
                        }
                        _ => continue,
                    },
                    Request::Shutdown => {
                        break;
                    }
                    _ => (),
                },
                Err(e) => {
                    match e {
                        // tolerate lagging
                        tokio::sync::broadcast::error::RecvError::Lagged(_) => continue,
                        tokio::sync::broadcast::error::RecvError::Closed => break,
                    }
                }
            }
        }

        Ok(())
    })
}

fn process_step() {
    //
}
