use serde::Deserialize;
use socketioxide::{
    extract::{Data, SocketRef},
    socket::DisconnectReason,
    SocketIo,
};

use crate::{stop_execution_pool, AppState};

#[derive(Deserialize)]
struct ButtonPress {
    direction: String,
    is_pressed: bool,
}

fn handle_button_press(state: &AppState, user_id: &str, press: ButtonPress) {
    if let Some(pool) = state.pools.lock().unwrap().get_mut(user_id) {
        if let Some(acc) = pool
            .simulation
            .lock()
            .unwrap()
            .controllable_acceleration
            .as_mut()
        {
            match press.direction.as_str() {
                "up" => acc.up = press.is_pressed,
                "down" => acc.down = press.is_pressed,
                "left" => acc.left = press.is_pressed,
                "right" => acc.right = press.is_pressed,
                _ => {}
            }
        }
    }
}

pub fn configure_socket_io(io: &SocketIo, state: AppState) {
    io.ns("/", move |socket_ref: SocketRef| {
        let state = state.clone();
        async move {
            let user_id = socket_ref.id.to_string();
            state
                .sockets
                .lock()
                .unwrap()
                .insert(user_id.clone(), socket_ref.clone());

            socket_ref.on("button_press", {
                let state = state.clone();
                let user_id = user_id.clone();
                move |_: SocketRef, Data(press): Data<ButtonPress>| async move {
                    handle_button_press(&state, &user_id, press);
                }
            });

            socket_ref.on_disconnect({
                let state = state.clone();
                let user_id = user_id.clone();
                move |_: SocketRef, _: DisconnectReason| async move {
                    stop_execution_pool(&state, &user_id);
                    state.sockets.lock().unwrap().remove(&user_id);
                }
            });
        }
    });
}
