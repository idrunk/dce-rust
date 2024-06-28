use futures_util::StreamExt;
use log::{error, info};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use dce_cli::protocol::CliRaw;
use dce_macro::api;
use dce_router::protocol::RoutableProtocol;
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_tokio_tungstenite::protocol::{SemiWebsocketProtocol, SemiWebsocketRaw};


/// `set RUST_LOG=debug && cargo run --bin app --target-dir target/websocket -- websocket start`
#[api("websocket/start")]
pub async fn websocket_start(req: CliRaw) {
    let addr = "0.0.0.0:2047";
    let server = TcpListener::bind(addr).await.unwrap();
    let router = Router::new()?
        .push(hello)
        .push(echo)
        .ready()?;

    info!("Dce started at {} with tokio-tungstenite", addr);

    while let Ok((stream, _)) = server.accept().await {
        tokio::spawn(async {
            let mut ws_stream = accept_async(stream)
                .await
                .expect("Error during the websocket handshake occurred");

            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(msg) => if msg.is_text() || msg.is_binary() {
                        SemiWebsocketProtocol::from(msg).binary().route(router.clone(), &mut ws_stream, Default::default()).await
                    },
                    Err(err) => error!("{err}")
                };
            }
        });
    }
    req.end(None)
}


/// `cargo run --bin app -- websocket 127.0.0.1:2047 -- hello`
#[api]
pub async fn hello(req: SemiWebsocketRaw) {
    req.pack(Serialized::String("hello world".to_string()))
}

/// `cargo run --bin app -- websocket 127.0.0.1:2047 -- echo "echo me"`
#[api("echo/{param?}")]
pub async fn echo(mut req: SemiWebsocketRaw) {
    let body = req.rp_mut().body().await?;
    let param = req.param("param")?.as_str().unwrap_or("");
    let body = format!(r#"path param data: "{}"{}body data: "{}""#, param, "\n", body);
    req.pack(Serialized::String(body))
}
