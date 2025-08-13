//! Leader manager task holds the leader state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::executor::{LocalExec, Signal};
use crate::leader::{State, Worker, WorkerExec};
use crate::worker::WorkerId;
use crate::{rpc, Error, Executor, Model, Result};

use super::{Config, LeaderId, Status};

pub type ManagerExec = LocalExec<Request, Result<Response>>;

impl ManagerExec {
    pub async fn get_config(&self) -> Result<Config> {
        let resp = self.execute(Request::GetConfig).await??;
        if let Response::GetConfig(config) = resp {
            Ok(config)
        } else {
            Err(Error::UnexpectedResponse(format!("")))
        }
    }

    pub async fn get_meta(&self) -> Result<LeaderId> {
        let resp = self.execute(Request::GetMeta).await??;
        if let Response::GetMeta(id) = resp {
            Ok(id)
        } else {
            Err(Error::UnexpectedResponse(format!("")))
        }
    }

    pub async fn get_status(&self) -> Result<Status> {
        let resp = self.execute(Request::Status).await??;
        if let Response::Status(status) = resp {
            Ok(status)
        } else {
            Err(Error::UnexpectedResponse(format!("")))
        }
    }

    pub async fn get_clock(&self) -> Result<usize> {
        let resp = self.execute(Request::GetClock).await??;
        if let Response::Clock(clock) = resp {
            Ok(clock)
        } else {
            Err(Error::UnexpectedResponse(format!("")))
        }
    }

    pub async fn get_random_worker(&self) -> Result<Worker> {
        use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};
        let workers = self.get_workers().await?;
        // TODO: consider different rngs.
        workers
            .into_values()
            .choose(&mut StdRng::from_os_rng())
            .ok_or(Error::Other(
                "leader: manager: failed selecting random worker".to_owned(),
            ))
    }

    pub async fn get_workers(&self) -> Result<FnvHashMap<WorkerId, Worker>> {
        let resp = self.execute(Request::GetWorkers).await??;
        if let Response::Workers(workers) = resp {
            Ok(workers)
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
        }
    }

    pub async fn add_worker(&self, worker: Worker) -> Result<()> {
        let resp = self.execute(Request::AddWorker(worker)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
        }
    }

    pub async fn remove_worker(&self, id: WorkerId) -> Result<()> {
        let resp = self.execute(Request::RemoveWorker(id)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
        }
    }

    pub async fn replace_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::PullModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
        }
    }

    pub async fn merge_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::MergeModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn get_model(&self) -> Result<Model> {
        let resp = self.execute(Request::GetModel).await??;
        if let Response::GetModel(model) = resp {
            Ok(model)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::SetModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn reorganize_entities(&self) -> Result<()> {
        let resp = self.execute(Request::ReorganizeEntities).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }
}

pub fn spawn(mut leader: State, cancel: CancellationToken) -> Result<ManagerExec> {
    use tokio_stream::StreamExt;

    let (exec, mut stream, _) = LocalExec::new(32);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some((req, snd)) = stream.next() => {
                    debug!("leader::manager: processing request: {req}");
                    let resp = handle_request(req, &mut leader).await;
                    snd.send(resp);
                },
                _ = cancel.cancelled() => break,
            }
        }
    });
    Ok(exec)
}

async fn handle_request(req: Request, mut leader: &mut State) -> Result<Response> {
    match req {
        Request::GetConfig => Ok(Response::GetConfig(leader.config.clone())),
        Request::GetMeta => Ok(Response::GetMeta(leader.id)),
        Request::Status => Ok(Response::Status(leader.status.clone())),
        Request::GetClock => Ok(Response::Clock(leader.clock)),
        Request::IncrementClock => {
            leader.clock += 1;
            Ok(Response::Clock(leader.clock))
        }
        Request::GetWorkers => Ok(Response::Workers(leader.workers.clone())),
        Request::AddWorker(worker) => {
            // If the leader was already initialize with a model, pass that
            // model to the newly added worker.
            if let Some(model) = &leader.model {
                let r = worker
                    .execute(Signal::from(rpc::worker::Request::SetModel(model.clone())))
                    .await?;
            }

            // Keep track of the worker handle internally
            leader.workers.insert(worker.id, worker);

            Ok(Response::Empty)
        }
        Request::RemoveWorker(id) => {
            leader.workers.remove(&id);
            Ok(Response::Empty)
        }
        Request::WorkerCount => todo!(),
        Request::GetModel => {
            if let Some(model) = &leader.model {
                Ok(Response::GetModel(model.clone()))
            } else {
                Err(Error::LeaderNotInitialized(format!("model not available")))
            }
        }
        Request::PullModel(model) => {
            // TODO: pulling the model is initiated by workers, perform
            // additional checks if they have permission to do this

            // TODO: should propagating of the model to the workers happen
            // on the manager level?

            leader.model = Some(model.clone());

            // propagate the model to workers
            // TODO: make concurrent
            for (worker_id, worker) in &leader.workers {
                worker
                    .execute(Signal::from(rpc::worker::Request::SetModel(model.clone())))
                    .await?;
            }

            Ok(Response::Empty)
        }
        Request::MergeModel(new_model) => {
            if let Some(model) = &mut leader.model {
                model.merge(new_model);

                // Propagate the model.
                for (worker_id, worker) in &leader.workers {
                    worker
                        .execute(Signal::from(rpc::worker::Request::SetModel(model.clone())))
                        .await?;
                }
            }
            Ok(Response::Empty)
        }
        Request::SetModel(model) => {
            leader.model = Some(model.clone());

            // Propagate the model.
            // TODO: make concurrent
            for (worker_id, worker) in &leader.workers {
                worker
                    .execute(Signal::from(rpc::worker::Request::SetModel(model.clone())))
                    .await?;
            }
            Ok(Response::Empty)
        }
        Request::ReorganizeEntities => {
            // println!("leader: manager: reorganizing entities");
            // // TODO: develop further.

            // // TODO: parallelize.
            // for (id, worker) in &leader.workers {
            //     worker
            //         .execute(rpc::worker::Request::MigrateEntities {})
            //         .await?;
            // }

            // for mut old_worker in leader.workers.values().cloned().collect::<Vec<_>>() {
            //     use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};
            //     let (_, mut new_worker) = leader
            //         .workers
            //         .iter_mut()
            //         .choose(&mut StdRng::from_os_rng())
            //         .unwrap();

            //     // worker
            //     //     .execute(rpc::worker::Request::MigrateEntities {})
            //     //     .await?;

            //     let resp = old_worker.execute(rpc::worker::Request::EntityList).await?;
            //     let entities = match resp {
            //         rpc::worker::Response::EntityList(list) => list,
            //         _ => unimplemented!(),
            //     };

            //     for entity in &entities {
            //         println!("despawning `{}` on {}", entity, old_worker.id);
            //         old_worker
            //             .execute(rpc::worker::Request::DespawnEntity {
            //                 name: entity.to_owned(),
            //             })
            //             .await?;
            //         println!("spawning instead on {}", new_worker.id);
            //         new_worker
            //             .execute(rpc::worker::Request::SpawnEntity {
            //                 name: entity.to_owned(),
            //                 prefab: Some(string::new_truncate("cube")),
            //             })
            //             .await?;
            //     }

            //     new_worker.entities = std::mem::take(&mut old_worker.entities);
            // }

            Ok(Response::Empty)
        }
    }
}

#[derive(Clone, strum::Display)]
pub enum Request {
    GetConfig,
    GetMeta,
    Status,
    GetClock,
    IncrementClock,
    GetWorkers,
    AddWorker(Worker),
    RemoveWorker(WorkerId),
    WorkerCount,
    GetModel,
    PullModel(Model),
    SetModel(Model),
    MergeModel(Model),
    ReorganizeEntities,
}

#[derive(Clone, strum::Display)]
pub enum Response {
    GetConfig(Config),
    GetMeta(LeaderId),
    Status(Status),
    Clock(usize),
    Workers(FnvHashMap<WorkerId, Worker>),
    // WorkerExecs(Vec<WorkerExec>),
    WorkerCount(usize),
    GetModel(Model),
    Empty,
}
