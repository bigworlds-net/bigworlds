use std::collections::HashMap;

use bigworlds::{EntityName, QueryProduct};
use ratatui::{style::Color, widgets::canvas::Shape};

/// Client's view of the simulated world.
///
/// Represented as a grid of cells.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct World {
    pub cubes: HashMap<EntityName, (i32, i32, i32)>,
}

impl From<bigworlds::QueryProduct> for World {
    fn from(product: bigworlds::QueryProduct) -> Self {
        let mut world = World::default();

        if let QueryProduct::NativeAddressedVar(map) = product {
            for ((ent, _comp, var_name), var) in map {
                world
                    .cubes
                    .entry(ent)
                    .and_modify(|c| match var_name.as_str() {
                        "x" => c.0 = var.to_int(),
                        "y" => c.1 = var.to_int(),
                        "z" => c.2 = var.to_int(),
                        _ => (),
                    })
                    .or_insert_with(|| match var_name.as_str() {
                        "x" => (var.to_int(), 0, 0),
                        "y" => (0, var.to_int(), 0),
                        "z" => (0, 0, var.to_int()),
                        _ => (0, 0, 0),
                    });
            }
        }

        world
    }
}

pub struct ProjectXZ<'a>(pub &'a World);
pub struct ProjectYZ<'a>(pub &'a World);

impl Shape for ProjectXZ<'_> {
    fn draw(&self, painter: &mut ratatui::widgets::canvas::Painter) {
        for cube in &self.0.cubes {
            painter.paint(cube.1.0 as usize, cube.1.1 as usize / 3, Color::White);
        }
    }
}

impl Shape for ProjectYZ<'_> {
    fn draw(&self, painter: &mut ratatui::widgets::canvas::Painter) {
        for cube in &self.0.cubes {
            painter.paint(cube.1.2 as usize, cube.1.1 as usize / 3, Color::White);
        }
    }
}
