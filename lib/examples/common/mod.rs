use bigworlds::{
    model::{
        behavior::{Behavior, BehaviorInner, InstancingTarget},
        Component, Entity, PrefabModel, Var as VarModel,
    },
    Model, Var, VarType,
};

/// Simple model definition for demonstration purposes.
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
