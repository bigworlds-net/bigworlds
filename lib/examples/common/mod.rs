use bigworlds::{
    model::{
        behavior::{Behavior, BehaviorInner, InstancingTarget},
        Component, Entity, PrefabModel, Var as VarModel,
    },
    string, Model, Var, VarType,
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
            name: string::new_truncate("cube"),
            components: vec![
                string::new_truncate("position"),
                string::new_truncate("color"),
            ],
        }],

        components: vec![
            Component {
                name: string::new_truncate("position"),
                vars: vec![
                    VarModel {
                        name: string::new_truncate("x"),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                    VarModel {
                        name: string::new_truncate("y"),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                    VarModel {
                        name: string::new_truncate("z"),
                        type_: VarType::Float,
                        default: Some(Var::Float(0.)),
                    },
                ],
            },
            Component {
                name: string::new_truncate("color"),
                vars: vec![VarModel {
                    name: string::new_truncate("value"),
                    type_: VarType::String,
                    default: Some(Var::String("blue".to_owned())),
                }],
            },
        ],
        entities: vec![
            Entity {
                name: string::new_truncate("cube_0"),
                prefab: Some(string::new_truncate("cube")),
                data: Default::default(),
            },
            Entity {
                name: string::new_truncate("cube_1"),
                prefab: Some(string::new_truncate("cube")),
                data: Default::default(),
            },
            Entity {
                name: string::new_truncate("cube_2"),
                prefab: Some(string::new_truncate("cube")),
                data: Default::default(),
            },
        ],
        behaviors: vec![
            Behavior {
                name: "shuffler".to_owned(),
                triggers: vec![string::new_truncate("step")],
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
                triggers: vec![string::new_truncate("step")],
                targets: vec![InstancingTarget::PerEntityWithAllComponents(vec![
                    string::new_truncate("color"),
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
