use mlua::{Lua, LuaSerdeExt, UserData, UserDataMethods};
use tokio_stream::StreamExt;

use crate::{
    address::LocalAddress, executor::Signal, rpc, Address, EntityName, Error, EventName, Executor,
    LocalExec, Result, Var, VarType,
};

use super::{BehaviorHandle, BehaviorTarget};

struct WorkerExec(
    LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
    Option<EntityName>,
);

impl UserData for WorkerExec {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("get", async |lua, this, address: String| {
            let addr = match address.parse() {
                Ok(addr) => addr,
                _ => {
                    if let Some(entity_context) = &this.1 {
                        match address.parse::<LocalAddress>() {
                            Ok(local_addr) => {
                                Address::from_local(local_addr, entity_context.clone())
                            }
                            _ => panic!("incorrect address"),
                        }
                    } else {
                        panic!("incorrect address")
                    }
                }
            };
            let response = this
                .0
                .execute(
                    Signal::from(rpc::worker::Request::GetVar(addr))
                        .originating_at(rpc::Caller::Behavior),
                )
                .await
                .unwrap()
                // .map_err(|e| e.into())?;
                .unwrap();
            if let rpc::worker::Response::GetVar(var) = response.into_payload() {
                match var.get_type() {
                    VarType::String => lua.to_value(&var.to_string()),
                    VarType::Int => lua.to_value(&var.to_int()),
                    VarType::Float => lua.to_value(&var.to_float()),
                    VarType::Bool => lua.to_value(&var.to_bool()),
                    _ => unimplemented!(),
                }
            } else {
                panic!("incorrect response from worker")
            }
        });
        methods.add_async_method(
            "set",
            async |lua, this, (address, var): (String, String)| {
                let addr = match address.parse() {
                    Ok(addr) => addr,
                    _ => {
                        if let Some(entity_context) = &this.1 {
                            match address.parse::<LocalAddress>() {
                                Ok(local_addr) => {
                                    Address::from_local(local_addr, entity_context.clone())
                                }
                                _ => panic!("incorrect address"),
                            }
                        } else {
                            panic!("incorrect address")
                        }
                    }
                };
                let var = Var::from_str(&var, Some(addr.var_type)).unwrap();
                let response = this
                    .0
                    .execute(
                        Signal::from(rpc::worker::Request::SetVar(addr, var))
                            .originating_at(rpc::Caller::Behavior),
                    )
                    .await
                    .unwrap()
                    // .map_err(|e| e.into())?;
                    .unwrap()
                    .into_payload();
                if let rpc::worker::Response::Empty = response {
                    lua.to_value(&())
                } else {
                    panic!("incorrect response from worker")
                }
            },
        );
        methods.add_async_method("invoke", async |lua, this, event: String| {
            tokio::spawn(async move {
                let response = this
                    .0
                    .execute(
                        Signal::from(rpc::worker::Request::Invoke {
                            events: vec![event],
                            global: true,
                        })
                        .originating_at(rpc::Caller::Behavior),
                    )
                    .await
                    .unwrap()
                    // .map_err(|e| e.into())?;
                    .unwrap();
                if let rpc::worker::Response::Empty = response.into_payload() {
                    ()
                } else {
                    panic!("incorrect response from worker")
                };
            });
            Ok(())
        });
        methods.add_async_method("worker_status", async |lua, this, ()| {
            let response = this
                .0
                .execute(
                    Signal::from(rpc::worker::Request::Status)
                        .originating_at(rpc::Caller::Behavior),
                )
                .await
                .unwrap()
                // .map_err(|e| e.into())?;
                .unwrap();
            if let rpc::worker::Response::Status {
                id,
                uptime,
                worker_count,
            } = response.into_payload()
            {
                lua.to_value(&(id, uptime, worker_count))
                // lua.to_value(&id)
            } else {
                panic!("incorrect response from worker")
            }
        });
    }
}

/// Prepares a lua instance, including exposing all the functionality
/// necessary to interoperate with the simulation cluster.
pub fn lua_instance(
    worker_exec: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
    entity_context: Option<EntityName>,
) -> Lua {
    let mut lua = Lua::new();

    let sim = WorkerExec(worker_exec, entity_context);
    lua.globals().set("sim", sim);

    lua
}

pub fn spawn(
    name: String,
    script: String,
    triggers: Vec<EventName>,
    target: BehaviorTarget,
    worker_exec: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
) -> Result<BehaviorHandle> {
    let _worker_exec = worker_exec.clone();
    let f = |mut behavior_stream: tokio_stream::wrappers::ReceiverStream<(
        Signal<rpc::behavior::Request>,
        tokio::sync::oneshot::Sender<crate::Result<Signal<rpc::behavior::Response>>>,
    )>,
             worker| {
        Box::pin(async move {
            let entity_context = match target {
                BehaviorTarget::Entity(name) => Some(name),
                _ => None,
            };
            let lua = lua_instance(_worker_exec.clone(), entity_context);
            let lua_fun = lua.load(script).into_function().unwrap();

            loop {
                tokio::select! {
                    Some((request, send)) = behavior_stream.next() => {
                        match request.into_payload() {
                            rpc::behavior::Request::Event(event) => {
                                lua_fun.call_async::<()>(()).await.unwrap();
                                send.send(Ok(Signal::from(rpc::behavior::Response::Empty)));
                            }
                            rpc::behavior::Request::Shutdown =>  {
                                send.send(Ok(Signal::from(rpc::behavior::Response::Empty)));
                                return Ok(());
                            }
                            _ => unimplemented!(),
                        }
                    }
                    else => {
                        continue;
                    }

                }
            }
            Ok(())
        }) as _
    };

    super::spawn_synced(f, name, triggers, worker_exec.clone())
}

pub fn spawn_unsynced(
    script: String,
    triggers: Vec<EventName>,
    broadcast: tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    exec: LocalExec<
        Signal<rpc::worker::Request>,
        std::result::Result<Signal<rpc::worker::Response>, Error>,
    >,
) -> Result<()> {
    let exec_ = exec.clone();
    let f = |mut receiver: tokio::sync::broadcast::Receiver<rpc::behavior::Request>, worker| {
        Box::pin(async move {
            // TODO: entity context.
            let lua = lua_instance(exec_, None);
            let lua_fun = lua.load(script).into_function().unwrap();

            loop {
                tokio::select! {
                    Ok(request) = receiver.recv() => {
                        match request {
                            rpc::behavior::Request::Event(event) => {
                                // println!("unsync lua behavior: processing event {}", event);
                                lua_fun.call_async::<()>(()).await.unwrap();
                            }
                            rpc::behavior::Request::Shutdown =>  {
                                return Ok(());
                            }
                            _ => unimplemented!(),
                        }
                    }
                    else => {
                        continue;
                    }

                }
            }
            Ok(())
        }) as _
    };

    super::spawn_unsynced(f, broadcast, exec)
}
