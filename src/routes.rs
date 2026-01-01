use std::{
    sync::{
        atomic::{AtomicBool, Ordering}, Arc,
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use nalgebra::Vector2;
use serde_json::{json, Value};
use socketioxide::extract::SocketRef;

use crate::{
    space_computation::{CollisionType, MovementType, Simulation, SpaceObject}, stop_execution_pool,
    AppState,
    SimulationExecutionPool,
};

pub async fn launch_simulation(
    State(state): State<AppState>,
    Json(data): Json<Value>,
) -> impl IntoResponse {
    let user_id = data["user_id"].as_str().unwrap_or_default().to_owned();
    stop_execution_pool(&state, &user_id);
    let simulation = Simulation::default();
    let time_delta = data["time_delta"].as_f64().unwrap_or(simulation.time_delta);
    let simulation_time = data["simulation_time"]
        .as_f64()
        .unwrap_or(simulation.simulation_time);
    let G = data["G"].as_f64().unwrap_or(simulation.g);
    let acceleration_rate = data["acceleration_rate"]
        .as_f64()
        .unwrap_or(simulation.acceleration_rate);
    let elasticity_coefficient = data["elasticity_coefficient"]
        .as_f64()
        .unwrap_or(simulation.elasticity_coefficient);
    let collision = data["collision_type"]
        .as_i64()
        .and_then(|v| CollisionType::try_from(v).ok())
        .unwrap_or(simulation.collision_type);

    let socket_ref = {
        let sockets = state.sockets.lock().unwrap();
        if let Some(socket_ref) = sockets.get(&user_id) {
            socket_ref.clone()
        } else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": "Socket is not connected" })),
            );
        }
    };

    let space_objects = {
        let mut space_objects = Vec::new();
        if let Some(space_objects_data) = data["space_objects"].as_array() {
            for object_data in space_objects_data {
                let position = Vector2::new(
                    object_data["position"]["x"].as_f64().unwrap_or(0.0),
                    object_data["position"]["y"].as_f64().unwrap_or(0.0),
                );
                let velocity = Vector2::new(
                    object_data["velocity"]["x"].as_f64().unwrap_or(0.0),
                    object_data["velocity"]["y"].as_f64().unwrap_or(0.0),
                );
                let movement_type =
                    MovementType::try_from(object_data["movement_type"].as_i64().unwrap_or(0))
                        .unwrap_or(MovementType::Static);

                let obj = match SpaceObject::new(
                    object_data["name"]
                        .as_str()
                        .unwrap_or("Unnamed")
                        .to_string(),
                    object_data["mass"].as_f64().unwrap_or(1.0),
                    object_data["radius"].as_f64().unwrap_or(1.0),
                    position,
                    velocity,
                    movement_type,
                ) {
                    Ok(obj) => obj,
                    Err(err) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(json!({ "status": "error", "message": err.to_string() })),
                        );
                    }
                };

                space_objects.push(obj);
            }
        }
        space_objects
    };

    let simulation = match Simulation::new(
        space_objects,
        time_delta,
        simulation_time,
        G,
        collision,
        acceleration_rate,
        elasticity_coefficient,
    ) {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": msg })),
            );
        }
    };

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);
    let simulation_clone = Arc::clone(&simulation);
    let socket_ref_clone = socket_ref.clone();

    let thread = thread::spawn(move || {
        simulate_loop(simulation_clone, stop_flag_clone, socket_ref_clone);
    });

    let pool = SimulationExecutionPool {
        simulation,
        stop_flag,
        thread,
    };

    state.pools.lock().unwrap().insert(user_id, pool);
    (StatusCode::OK, Json(json!({ "status": "success" })))
}

pub async fn delete_simulation(
    State(state): State<AppState>,
    Json(data): Json<Value>,
) -> impl IntoResponse {
    let user_id = data["user_id"].as_str().unwrap_or_default().to_string();
    stop_execution_pool(&state, &user_id);
    Json(json!({ "status": "success" }))
}

fn simulate_loop(
    simulation: Arc<Mutex<Simulation>>,
    stop_flag: Arc<AtomicBool>,
    socket_ref: SocketRef,
) {
    let target_step_time = 1.0 / 60.0;

    let (steps_per_emit, total_steps) = {
        let simulation_guard = simulation.lock().unwrap();
        let steps = (target_step_time / simulation_guard.time_delta)
            .max(1.0)
            .floor() as usize;
        let total =
            (simulation_guard.simulation_time / simulation_guard.time_delta).floor() as usize;
        (steps, total)
    };

    let mut step_count = 0;

    while !stop_flag.load(Ordering::Relaxed) && step_count < total_steps {
        let start = Instant::now();

        for _ in 0..steps_per_emit {
            if stop_flag.load(Ordering::Relaxed) || step_count >= total_steps {
                break;
            }

            let mut simulation_guard = simulation.lock().unwrap();
            simulation_guard.calculate_step();
            step_count += 1;
        }

        let snapshot = {
            let simulation_guard = simulation.lock().unwrap();
            simulation_guard
                .space_objects
                .iter()
                .enumerate()
                .map(|(i, obj)| {
                    json!({
                        i.to_string(): {
                            "x": obj.position.x,
                            "y": obj.position.y,
                            "radius": obj.radius,
                        }
                    })
                })
                .collect::<Vec<_>>()
        };

        let payload = serde_json::to_string(&snapshot).unwrap_or_else(|_| "[]".to_string());
        let _ = socket_ref.emit("update_step", &payload);

        if let Some(remaining) =
            Duration::from_secs_f64(target_step_time).checked_sub(start.elapsed())
        {
            thread::sleep(remaining);
        }
    }
}
