use futures_util::StreamExt;
use log::info;
use tokio::net::TcpListener;
use tokio_util::bytes::BytesMut;
use tokio_util::codec::{BytesCodec, Decoder};
use dce_cli::protocol::CliRaw;
use dce_macro::api;
use dce_router::protocol::RoutableProtocol;
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_tokio::protocol::{SemiTcpProtocol, SemiTcpRaw};

/// `set RUST_LOG=debug && cargo run --bin app --target-dir target/tcp -- tcp start`
#[api("tcp/start")]
pub async fn tcp_start(req: CliRaw) {
    let addr = "0.0.0.0:2048";
    let server = TcpListener::bind(addr).await.unwrap();
    let router = Router::new()?
        .push(hello)
        .push(echo)
        .ready()?;

    info!("Dce started at {} with tokio-tcp", addr);

    while let Ok((stream, _)) = server.accept().await {
        tokio::spawn(async {
            let framed = BytesCodec::new().framed(stream);
            let (mut frame_writer, mut frame_reader) = framed.split::<BytesMut>();

            while let Some(msg) = frame_reader.next().await {
                match msg {
                    Ok(str) => SemiTcpProtocol::from(str).route(router.clone(), &mut frame_writer, Default::default()).await,
                    Err(err) => println!("Socket closed with error: {:?}", err),
                };
            }
        });
    }
    req.end(None)
}


/// `cargo run --bin app -- tcp 127.0.0.1:2048 -- hello`
#[api]
pub async fn hello(req: SemiTcpRaw) {
    req.pack(Serialized::String("hello world".to_string()))
}

/// `cargo run --bin app -- tcp 127.0.0.1:2048 -- echo "echo me"`
#[api("echo/{param?}")]
pub async fn echo(mut req: SemiTcpRaw) {
    let body = req.rp_mut().body().await?;
    let param = req.param("param")?.as_str().unwrap_or("");
    let body = format!(r#"path param data: "{}"{}body data: "{}""#, param, "\n", body);
    req.pack(Serialized::String(body))
}
