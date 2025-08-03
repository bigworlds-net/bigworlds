//! Physics solver service using the `rapier3d` physics library.

#![allow(unused)]

use std::time::Instant;

use bigworlds::{
    Executor, Query, Result, Var,
    client::{self, Client, CompressionPolicy, r#async::AsyncClient},
    query::{Description, Filter, Map, Scope},
    rpc::{
        self,
        msg::{self, Message, StatusRequest},
    },
    string,
};
// use bigworlds_client::Client;

use fnv::FnvHashMap;
use rapier3d::prelude::*;

use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = client::Config {
        name: "rapier_solver".to_owned(),
        is_blocking: true,
        compress: CompressionPolicy::Nothing,
        ..Default::default()
    };
    let mut client = Client::connect("127.0.0.1:9123".parse()?, config).await?;
    println!("connected");

    // Setup the scene valid for this particular example.

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider = ColliderBuilder::cuboid(1000.0, 0.1, 1000.0).build();
    collider_set.insert(collider);

    // Create structures necessary for the simulation.
    let gravity = vector![0.0, -9.81, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = DefaultBroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let mut query_pipeline = QueryPipeline::new();
    let physics_hooks = ();
    let event_handler = ();

    // Initialize the cubes as they exist in the world
    let query = Query {
        filters: vec![Filter::AllComponents(vec![
            string::new_truncate("position"),
            string::new_truncate("rotation"),
        ])],
        scope: Scope::Global,
        mappings: vec![Map::Component(string::new_truncate("position"))],
        description: Description::Addressed,
        ..Default::default()
    };
    let product = client.query(query).await?.to_map()?;
    println!("cube count: {}", product.len());

    let mut index_to_entity = FnvHashMap::default();

    let mut cubes = FnvHashMap::default();
    for (cube_addr, _) in &product {
        if !cubes.contains_key(&cube_addr.entity) {
            cubes.insert(cube_addr.entity.clone(), vec![]);
        }
    }
    for (cube_addr, cube_var) in &product {
        cubes
            .get_mut(&cube_addr.entity)
            .unwrap()
            .push((cube_addr, cube_var));
    }
    for (cube_name, cube_vars) in cubes {
        let z = cube_vars
            .iter()
            .find(|(addr, var)| addr.var_name.as_str() == "z")
            .map(|(_, var)| var)
            .unwrap();
        let x = cube_vars
            .iter()
            .find(|(addr, var)| addr.var_name.as_str() == "x")
            .map(|(_, var)| var)
            .unwrap();
        let y = cube_vars
            .iter()
            .find(|(addr, var)| addr.var_name.as_str() == "y")
            .map(|(_, var)| var)
            .unwrap();

        // Create a rigid body for the cube.
        let rigid_body = RigidBodyBuilder::dynamic()
            .translation(vector![
                *x.as_float().unwrap(),
                *y.as_float().unwrap(),
                *z.as_float().unwrap(),
            ])
            .build();
        let collider = ColliderBuilder::cuboid(0.5, 0.5, 0.5)
            .restitution(0.5)
            .build();
        let cube_body_handle = rigid_body_set.insert(rigid_body);
        collider_set.insert_with_parent(collider, cube_body_handle, &mut rigid_body_set);
        index_to_entity.insert(cube_body_handle, cube_name);
    }

    let client_ = client.clone();
    tokio::spawn(async move {
        let mut step = 0;
        loop {
            let resp = client_
                .execute(rpc::msg::Message::AdvanceRequest(msg::AdvanceRequest {
                    step_count: 1,
                    wait: true,
                }))
                .await?;
            step += 1;

            // Query data from the world.

            // Solve tick, mutate.

            let now = Instant::now();
            physics_pipeline.step(
                &gravity,
                &integration_parameters,
                &mut island_manager,
                &mut broad_phase,
                &mut narrow_phase,
                &mut rigid_body_set,
                &mut collider_set,
                &mut impulse_joint_set,
                &mut multibody_joint_set,
                &mut ccd_solver,
                Some(&mut query_pipeline),
                &physics_hooks,
                &event_handler,
            );
            println!("physics step: {}ms", now.elapsed().as_millis());

            let now = Instant::now();
            let mut vars = vec![];
            for (handle, body) in rigid_body_set.iter() {
                let entity = index_to_entity.get(&handle).unwrap();
                // println!("{}", body.translation().y);
                // HACK
                vars.push((
                    format!("{}.position.float.y", entity).parse()?,
                    Var::Float(body.translation().y),
                ));
                vars.push((
                    format!("{}.position.float.x", entity).parse()?,
                    Var::Float(body.translation().x),
                ));
                vars.push((
                    format!("{}.position.float.z", entity).parse()?,
                    Var::Float(body.translation().z),
                ));
                vars.push((
                    format!("{}.rotation.float.x", entity).parse()?,
                    Var::Float(body.rotation().i),
                ));
                vars.push((
                    format!("{}.rotation.float.y", entity).parse()?,
                    Var::Float(body.rotation().j),
                ));
                vars.push((
                    format!("{}.rotation.float.z", entity).parse()?,
                    Var::Float(body.rotation().k),
                ));
                vars.push((
                    format!("{}.rotation.float.w", entity).parse()?,
                    Var::Float(body.rotation().w),
                ));
            }
            println!("setting vars internal: {}ms", now.elapsed().as_millis());

            let now = Instant::now();
            client_.pull(vars).await?;
            println!("setting vars upload: {}ms", now.elapsed().as_millis());
            println!("step: {}", step);

            // let (_, body) = rigid_body_set.iter().next().unwrap();
            // println!(
            //     "(step {step}) first cube: altitude: {}",
            //     body.translation().y
            // );

            // break;
            // if step >= 10 {
            // break;
            // }
        }
        Result::<()>::Ok(())
    });

    tokio::signal::ctrl_c().await;

    let resp = client.execute(Message::Disconnect).await?;
    println!("disconnect response: {:?}", resp);

    Ok(())
}
