use bigworlds::{
    leader,
    model::{
        behavior::{Behavior, BehaviorInner, InstancingTarget},
        Component, Entity, PrefabModel, Var as VarModel,
    },
    server,
    sim::SimConfig,
    worker, Model, SimHandle, Var, VarType,
};
use itertools::Itertools;
use tokio_util::sync::CancellationToken;

/// Generates a new cluster of certain size with nodes organized into a ring.
///
/// # Notes
///
/// Since the leader is required for handling certain operations, putting it
/// on a ring and having workers access it via other workers' proxying the
/// request, can be quite inefficient.
///
/// Changing up the classic ring topology a little and introducing one or more
/// cross-ring connections may be desirable.
///
/// # Diagram
///
/// ```ignore
///      #l
///   #w6 \ #w0
/// #w5  / \  #w1
///   #w4   #w2
///      #w3s
///
/// # == participant on the ring
/// l == leader
/// w == worker
/// s == server
/// \ == optional cross-ring connections
///
/// ```
pub async fn ring_cluster(size: usize) -> anyhow::Result<(leader::Handle, Vec<worker::Handle>)> {
    let cancel = CancellationToken::new();

    // Spawn leader.
    let mut leader = leader::spawn(Default::default(), cancel.clone())?;

    // Spawn workers.
    let mut workers = vec![];
    for _ in 0..size {
        let worker = worker::spawn(worker_config(), cancel.clone())?;
        workers.push(worker);
    }

    // Connect all workers iterating forward.
    for (a, b) in workers.iter().tuple_windows() {
        a.connect_to_local_worker(b).await?;
    }

    // Close the ring by connecting the leader to first and last worker.
    leader
        .connect_to_local_worker(workers.first().unwrap(), true)
        .await?;
    leader
        .connect_to_local_worker(workers.last().unwrap(), true)
        .await?;

    // Establish a server with one of the workers.
    // let server = server::spawn(Default::default(), cancel.clone())?;
    // server.connect_to_worker(&worker_3, true).await?;

    Ok((leader, workers))
}

/// Generates a new cluster of certain size with nodes organized into a star.
/// Leader is the central node and is connected to all workers. Every worker
/// is directly connected only to the central leader and none of the other
/// workers.
///
/// Cluster size describes the number of workers.
pub async fn star_cluster(size: usize) -> anyhow::Result<(leader::Handle, Vec<worker::Handle>)> {
    let cancel = CancellationToken::new();

    // Spawn leader.
    let leader = leader::spawn(Default::default(), cancel.clone())?;

    // Spawn workers.
    let mut workers = vec![];
    for _ in 0..size {
        // Disable entity archive for faster startup times.
        let worker = worker::spawn(worker_config(), cancel.clone())?;
        // Connect to leader.
        worker.connect_to_local_leader(&leader).await?;

        workers.push(worker);
    }

    Ok((leader, workers))
}

/// Common test-friendly configuration for the local sim instance.
pub fn sim_config() -> SimConfig {
    SimConfig {
        worker: worker_config(),
        ..Default::default()
    }
}

pub fn sim_config_single_worker() -> SimConfig {
    SimConfig {
        worker_count: 1,
        worker: worker_config(),
        ..Default::default()
    }
}

/// Common test-friendly configuration for the worker.
pub fn worker_config() -> worker::Config {
    // Disable entity archive for faster startup times.
    worker::Config::no_archive()
}

/// Simple model definition for testing purposes.
///
/// # Model contents
///
/// We describe a 3-dimensional world populated with cubes characterized
/// by their position in the world and a particular color.
///
/// Cube color depends on what worker the cube entity belongs to currently.
pub fn model() -> Model {
    Model {
        prefabs: vec![PrefabModel {
            name: "cube".to_owned(),
            components: vec![
                "position".to_owned(),
                "color".to_owned(),
            ],
        }],

        components: vec![
            Component {
                name: "position".to_owned(),
                vars: vec![
                    VarModel {
                        name: "x".to_owned(),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                    VarModel {
                        name: "y".to_owned(),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                    VarModel {
                        name: "z".to_owned(),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                ],
            },
            Component {
                name: "color".to_owned(),
                vars: vec![VarModel {
                    name: "value".to_owned(),
                    type_: VarType::String,
                    default: Some(Var::String("blue".to_owned())),
                }],
            },
        ],
        entities: vec![
            Entity {
                name: "cube_0".to_owned(),
                prefab: Some("cube".to_owned()),
                data: Default::default(),
            },
            Entity {
                name: "cube_1".to_owned(),
                prefab: Some("cube".to_owned()),
                data: Default::default(),
            },
            Entity {
                name: "cube_2".to_owned(),
                prefab: Some("cube".to_owned()),
                data: Default::default(),
            },
        ],
        behaviors: vec![
            Behavior {
                name: "shuffler".to_owned(),
                triggers: vec!["step".to_owned()],
                targets: vec![InstancingTarget::GlobalSingleton],
                tracing: "".to_owned(),
                inner: BehaviorInner::Lua {
                    synced: true,
                    script: r#"
                    print("lua: shuffler: " .. sim:worker_status()[1])
                "#
                    .to_owned(),
                },
            },
            Behavior {
                name: "greeter".to_owned(),
                triggers: vec!["step".to_owned()],
                targets: vec![InstancingTarget::PerEntityWithAllComponents(vec![
                    "color".to_owned(),
                ])],
                tracing: "".to_owned(),
                inner: BehaviorInner::Lua {
                    synced: true,
                    script: r#"
                    sim:set("position.float.x", sim:get("position.float.x") + 1)
                    print("lua: greeter: position.float.x == " .. sim:get("position.float.x") .. ", at worker: " .. sim:worker_status()[1])
                "#
                    .to_owned(),
                },
            },
        ],
        ..Default::default()
    }
}
