//! Tests of the querying mechanism.

use tokio_util::sync::CancellationToken;

use bigworlds::behavior::BehaviorTarget;
use bigworlds::client::r#async::AsyncClient;
use bigworlds::client::{self, Client};
use bigworlds::query::{Description, Filter, Map};
use bigworlds::sim::SimConfig;
use bigworlds::{model, ServerConfig, Signal, Var, VarType};
use bigworlds::{rpc, Executor, Model, Query, QueryProduct};

#[allow(unused)]
mod common;

// TODO: resolve issue with client unable to connect.
#[tokio::test]
async fn client_net_query() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Spawn a local sim instance with a server listening at a selected localhost
    // port.
    let mut sim = bigworlds::sim::spawn_with(
        SimConfig {
            server: Some(ServerConfig {
                listeners: vec!["quic://[::1]:9123".parse()?],
                ..Default::default()
            }),
            ..common::sim_config_single_worker()
        },
        CancellationToken::new(),
    )
    .await?;

    // Define a basic model of monsters with health and stamina.
    let mut model = Model::default();
    model.components.push(model::Component {
        name: "health".parse()?,
        vars: vec![model::Var {
            name: "current".parse()?,
            type_: VarType::Int,
            default: Some(Var::Int(rand::random_range(1..100))),
        }],
    });
    model.components.push(model::Component {
        name: "stamina".parse()?,
        vars: vec![model::Var {
            name: "current".parse()?,
            type_: VarType::Float,
            default: Some(Var::Float(rand::random_range(5.0..50.0))),
        }],
    });
    model.prefabs.push(model::PrefabModel {
        name: "monster".parse()?,
        components: vec!["health".parse()?, "stamina".parse()?],
    });

    // Pull the model and initialize.
    sim.pull_model(model).await?;
    sim.initialize().await?;

    // Spawn a couple of different entities based on the `monster` prefab.
    for name in ["ghoul", "wyvern", "troll"] {
        sim.spawn_entity(name.parse()?, Some("monster".parse()?))
            .await?;
    }

    // Explicitly set the `stamina` value of the `ghoul` entity.
    sim.set_var(
        "ghoul.stamina.float.current".parse()?,
        Var::from_str("101.0", Some(VarType::Float))?,
    )
    .await?;

    // Spawn a behavior task.
    let _ = sim
        .spawn_behavior_synced(
            |_, exec| {
                Box::pin(async move {
                    // Inside the behavior we define a simple loop mutating
                    // the `stamina` value of the `ghoul` entity. Specifically,
                    // we're decreasing the value by one ten times a second.
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                        // Execute our query using the available worker
                        // executor.
                        let product: QueryProduct = exec
                            .execute(
                                Signal::from(rpc::worker::Request::ProcessQuery(
                                    Query::default()
                                        .filter(Filter::Name(vec!["ghoul".parse().unwrap()]))
                                        .map(Map::Components(vec!["stamina".parse().unwrap()])),
                                ))
                                .originating_at(rpc::Caller::Behavior),
                            )
                            .await??
                            .into_payload()
                            .try_into()?;

                        // Process the resulting product and mutate the
                        // desired value.
                        let mut product = product.to_vec();
                        let var = product.pop().unwrap();
                        let var = Var::Float(var.to_float() - 1.);

                        // Send the updated value back through the worker
                        // interface.
                        exec.execute(
                            rpc::worker::Request::SetVar(
                                "ghoul.stamina.float.current".parse()?,
                                var,
                            )
                            .into(),
                        )
                        .await??;
                    }
                })
            },
            // Define this behavior to be required to run on each worker in the
            // cluster.
            BehaviorTarget::Worker,
            // Don't subscribe to any external event triggering.
            vec![],
        )
        .await?;

    println!("before connected");
    // Connect to the cluster through the client interface.
    let mut client =
        Client::connect("quic://[::1]:9123".parse()?, client::Config::default()).await?;

    println!("connected");

    // Specify the query. We only want entities with health and stamina values
    // in the specified range. We also make sure to only return stamina values
    // through the use of proper mapping entry.
    let query = Query::default()
        .description(Description::Addressed)
        .filter(Filter::VarRange(
            "health.int.current".parse()?,
            Var::Int(1),
            Var::Int(100),
        ))
        // Notice this var range filter. As we execute the query over time,
        // since the stamina value is constantly falling, we will end up
        // reaching the low end of the filter eventually.
        .filter(Filter::VarRange(
            "stamina.float.current".parse()?,
            Var::Float(70.),
            Var::Float(102.),
        ))
        .map(Map::components(vec!["stamina"]));

    for _ in 0..5 {
        // Issue the query and receive back the resulting product.
        let query_product = client.query(query.clone()).await?;

        // Process the query product.
        if let QueryProduct::AddressedVar(product) = query_product {
            if product.is_empty() {
                println!("query product is empty");
            }
            for data_point in product {
                let (addr, var) = data_point;
                println!("{}: {:?}", addr, var);
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    }

    sim.shutdown().await?;

    Ok(())
}
