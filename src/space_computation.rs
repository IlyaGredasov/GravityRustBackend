use std::{error::Error, fmt};

use nalgebra::Vector2;
use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive)]
#[repr(i64)]
pub enum MovementType {
    Static = 0,
    Ordinary = 1,
    Controllable = 2,
}

#[derive(Debug, Clone)]
pub struct SpaceObject {
    pub name: String,
    pub mass: f64,
    pub radius: f64,
    pub position: Vector2<f64>,
    pub velocity: Vector2<f64>,
    pub acceleration: Vector2<f64>,
    pub movement_type: MovementType,
}

impl SpaceObject {
    pub fn new(
        name: impl Into<String>,
        mass: f64,
        radius: f64,
        position: Vector2<f64>,
        velocity: Vector2<f64>,
        movement_type: MovementType,
    ) -> Result<Self, Box<dyn Error>> {
        if mass <= 0.0 {
            return Err("Mass must be positive".into());
        }
        if radius <= 0.0 {
            return Err("Radius must be positive".into());
        }

        let velocity = match movement_type {
            MovementType::Static => Vector2::new(0.0, 0.0),
            _ => velocity,
        };

        Ok(Self {
            name: name.into(),
            mass,
            radius,
            position,
            velocity,
            acceleration: Vector2::new(0.0, 0.0),
            movement_type,
        })
    }
}

impl fmt::Display for SpaceObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SpaceObject({}, mass:{}, radius:{}, position:{:?}, velocity:{:?}, acceleration:{:?}, MovementType={:?})",
            self.name,
            self.mass,
            self.radius,
            self.position,
            self.velocity,
            self.acceleration,
            self.movement_type
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive)]
#[repr(i64)]
pub enum CollisionType {
    Traversing = 0,
    Elastic = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ControllableAcceleration {
    pub right: bool,
    pub left: bool,
    pub up: bool,
    pub down: bool,
}

pub fn calculate_new_normal_velocity(
    m1: f64,
    m2: f64,
    v1: Vector2<f64>,
    v2: Vector2<f64>,
    e: f64,
) -> Vector2<f64> {
    let mut result = Vector2::new(0.0, 2.0);
    for i in 0..2 {
        result[i] = ((m1 - e * m2) * v1[i] + (1.0 + e) * m2 * v2[i]) / (m1 + m2);
    }
    result
}

fn maybe_update_velocity(
    movement_type: MovementType,
    own_mass: f64,
    other_mass: f64,
    own_v: Vector2<f64>,
    other_v: Vector2<f64>,
    elasticity: f64,
) -> Vector2<f64> {
    if movement_type != MovementType::Static {
        calculate_new_normal_velocity(own_mass, other_mass, own_v, other_v, elasticity)
    } else {
        own_v
    }
}

#[derive(Clone)]
pub struct Simulation {
    pub space_objects: Vec<SpaceObject>,
    pub time_delta: f64,
    pub simulation_time: f64,
    pub g: f64,
    pub collision_type: CollisionType,
    pub acceleration_rate: f64,
    pub elasticity_coefficient: f64,
    pub controllable_acceleration: Option<ControllableAcceleration>,
}

impl Default for Simulation {
    fn default() -> Self {
        Simulation::new(vec![], 1e-5, 10.0, 10.0, CollisionType::Elastic, 1.0, 0.5).unwrap()
    }
}

impl Simulation {
    pub fn new(
        space_objects: Vec<SpaceObject>,
        time_delta: f64,
        simulation_time: f64,
        g: f64,
        collision_type: CollisionType,
        acceleration_rate: f64,
        elasticity_coefficient: f64,
    ) -> Result<Self, String> {
        if space_objects
            .iter()
            .filter(|o| o.movement_type == MovementType::Controllable)
            .count()
            > 1
        {
            return Err("Multiple controllable objects are not supported".into());
        }
        if time_delta <= 0.0 {
            return Err("Time delta must be positive".into());
        }
        if simulation_time <= 0.0 {
            return Err("Simulation time must be positive".into());
        }
        if g <= 0.0 {
            return Err("Gravity constant must be positive".into());
        }
        if acceleration_rate <= 0.0 {
            return Err("Acceleration rate must be positive".into());
        }
        if elasticity_coefficient < 0.0 || elasticity_coefficient > 1.0 {
            return Err("Elasticity coefficient must be in [0, 1]".into());
        }

        let controllable_acceleration = if space_objects
            .iter()
            .any(|o| o.movement_type == MovementType::Controllable)
        {
            Some(ControllableAcceleration::default())
        } else {
            None
        };

        Ok(Self {
            space_objects,
            time_delta,
            simulation_time,
            g,
            collision_type,
            acceleration_rate,
            elasticity_coefficient,
            controllable_acceleration,
        })
    }
    pub fn calculate_collisions(&mut self) {
        let mut collisions = Vec::new();

        for i in 0..self.space_objects.len() {
            for j in (i + 1)..self.space_objects.len() {
                let delta_pos = self.space_objects[j].position - self.space_objects[i].position;
                let distance = delta_pos.norm();
                let min_distance = self.space_objects[i].radius + self.space_objects[j].radius;

                if distance <= min_distance {
                    collisions.push((i, j));
                }
            }
        }

        for (i, j) in collisions {
            let delta_pos = self.space_objects[j].position - self.space_objects[i].position;
            let normal = delta_pos.normalize();
            let tangent = Vector2::new(-normal.y, normal.x);

            let v_i = self.space_objects[i].velocity;
            let v_j = self.space_objects[j].velocity;

            let v_i_n = v_i.dot(&normal);
            let v_i_t = v_i.dot(&tangent);
            let v_j_n = v_j.dot(&normal);
            let v_j_t = v_j.dot(&tangent);

            let v_i_n_vec = v_i_n * normal;
            let v_i_t_vec = v_i_t * tangent;
            let v_j_n_vec = v_j_n * normal;
            let v_j_t_vec = v_j_t * tangent;

            let new_v_i_n_vec = maybe_update_velocity(
                self.space_objects[i].movement_type,
                self.space_objects[i].mass,
                self.space_objects[j].mass,
                v_i_n_vec,
                v_j_n_vec,
                self.elasticity_coefficient,
            );

            let new_v_j_n_vec = maybe_update_velocity(
                self.space_objects[j].movement_type,
                self.space_objects[j].mass,
                self.space_objects[i].mass,
                v_j_n_vec,
                v_i_n_vec,
                self.elasticity_coefficient,
            );

            self.space_objects[i].velocity = new_v_i_n_vec + v_i_t_vec;
            self.space_objects[j].velocity = new_v_j_n_vec + v_j_t_vec;
        }
    }

    pub fn calculate_acceleration(&self, i: usize) -> Vector2<f64> {
        let obj_i = &self.space_objects[i];

        if obj_i.movement_type == MovementType::Static {
            return Vector2::zeros();
        }

        let mut acceleration = Vector2::zeros();

        for (j, obj_j) in self.space_objects.iter().enumerate() {
            if i == j {
                continue;
            }

            let r_vec = obj_j.position - obj_i.position;
            let r_norm = r_vec.norm();

            if r_norm == 0.0 {
                continue;
            }

            acceleration += self.g * obj_j.mass / r_norm.powf(1.5) * r_vec;
        }

        if obj_i.movement_type == MovementType::Controllable {
            if let Some(ctrl) = &self.controllable_acceleration {
                let direction = Vector2::new(
                    f64::from(ctrl.right) - f64::from(ctrl.left),
                    f64::from(ctrl.up) - f64::from(ctrl.down),
                );
                acceleration += self.acceleration_rate * direction;
            }
        }

        acceleration
    }

    pub fn calculate_step(&mut self) {
        if self.collision_type == CollisionType::Elastic {
            self.calculate_collisions();
        }

        let mut new_space_objects = self.space_objects.clone();

        for i in 0..self.space_objects.len() {
            if self.space_objects[i].movement_type != MovementType::Static {
                let acceleration = self.calculate_acceleration(i);
                new_space_objects[i].acceleration = acceleration;
                new_space_objects[i].position += self.space_objects[i].velocity * self.time_delta;
                new_space_objects[i].velocity += acceleration * self.time_delta;
            }
        }

        self.space_objects = new_space_objects;
    }
}
