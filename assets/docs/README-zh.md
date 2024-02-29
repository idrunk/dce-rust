几乎所有应用程序都提供交互接口，不同类型的接口由于协议不同，没有统一的路由规则，可能造成程序结构混乱，无法统一、便捷的对各接口进行管控。为了解决这个问题，DCE提供了一个标准路由器，并封装了可路由协议特征，对于没有标准URI路径来定位的接口，只要实现可路由协议特征，即可通过DCE路由器来统一路由。

> *DCE本是一个PHP编写的网络编程框架，集成了路由器、HTTP/TCP等服务器及一些其他功能。由于迷恋上RUST，作者将其搬了过来。目前仅搬了较具特色的路由器，未来还会将会话管理器搬过来，其他大部分是一些轮子功能，不会搬过来了。*

DCE路由器包位于`crates/router`，可路由协议的实现样例位于`crates/protocols`下，而`src`目录下是DCE路由器的应用示例代码，下述是目录结构说明图：

```
[ROOT]
├─assets                                    资源目录
│  ├─templates                              html等模板目录
├─crates                                    箱包目录
│  ├─macro                                  DCE宏包
│  ├─protocols                              可路由协议实现包目录
│  │  ├─cli                                 cli可路由协议实现
│  │  ├─hyper                               hyper http可路由协议实现
│  │  ├─tokio                               tokio tcp/udp可路由协议实现（此实现为样例代码，不推荐直接在实际项目中应用）
│  │  ├─tokio-tungstenite                   tokio tungstenite websocket可路由协议实现（此实现为样例代码，不推荐直接在实际项目中应用）
│  ├─router                                 DCE路由器包
│  ├─util                                   DCE工具包
├─src                                       DCE应用示例代码
│  ├─apis                                   路由接口示例代码
```

关于可路由协议特征，由于http与cli有标准可用于路由的路径地址，所以这些实现通用性较强，基本可用于实际项目。其他的如tcp可路由协议的实现，目前主要作为样例代码，因为一般tcp等通信，都是在之上定义一个新的业务协议，它们没有统一的标准，所以需要用户自行实现相应的可路由协议来适配DCE路由器。

**DCE路由器除了基本的路由功能外，还提供了全局控制器前置事件接口，Request对象上提供了数据转换与序列化工具接口：**
- 全局控制器前置事件接口可以做一些前置工作，如全局权限控制，在这里做会非常方便。
- 数据转换器，用于`DTO`与`ENTITY`间转换，通过实现`From/Into`特征，可以对实体数据进行脱敏等操作，转换为利于传输的数据结构。
- 序列化接口，用于将`DTO`编码为`序列`以便传输，或将`序列`解析为`DTO`以便转换为实体对象，具体序列化工具通过`api`宏配置。

#### 路由性能

由于流程链非常短，以路径匹配API后就直接调用控制器，性能非常高。对于普通路径API，是直接以路径从API哈希表匹配，时间复杂度为O(1)。对于变量路径API，若路径中只有一个变量，则时间复杂度为O(n)，若有多个，则指数级增加。所以建议使用普通路径，或者保证变量路径中的变量数尽可能少，或者保证同辈变量尽可能少，以便获取最高的路由性能。

#### 完整路由流程图：

![Router flow](dce-router-flow.svg)


#### DCE路由应用示例：

*更多完整示例，请至[src](../../src)目录下查看，更完整文档点[这里](https://docs.rs/dce)查看。*

```rust
use rand::random;
use serde::Serialize;
use dce_cli::protocol::{CliProtocol, CliConvert, CliRaw};
use dce_macro::{api, openly_err};
use dce_router::router::Router;
use dce_router::serializer::JsonSerializer;

#[tokio::main]
async fn main() {
    let router = Router::new()
        .push(hello)
        .ready();

    CliProtocol::new(1).route(router.clone(), Default::default()).await;
}

/// `cargo run --package dce --bin app -- hello`
/// `cargo run --package dce --bin app -- hello DCE`
#[api("hello/{target?}")]
pub async fn hello(req: CliRaw) {
    let target = req.param("target")?.get().unwrap_or("RUST").to_owned();
    req.raw_resp(format!("Hello {} !", target))
}

/// `cargo run --package dce --bin app -- session`
/// `cargo run --package dce --bin app -- session --user DCE`
#[api(serializer = JsonSerializer{})]
pub async fn session(mut req: CliConvert<User, UserDto>) {
    let name = req.rpi_mut().args_mut().remove("--user").ok_or_else(|| openly_err!(r#"please pass in the "--user" arg"#))?;
    let resp = User {
        nickname: name.clone(),
        gender: if random::<u8>() > 127 { Gender::Female } else { Gender::Male },
        name,
        cellphone: "+8613344445555".to_owned(),
    };
    req.resp(resp)
}

#[derive(Serialize)]
pub enum Gender {
    Male,
    Female,
}

pub struct User {
    nickname: String,
    gender: Gender,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    cellphone: String,
}

#[derive(Serialize)]
pub struct UserDto {
    nickname: String,
    gender: Gender,
}

impl From<User> for UserDto {
    fn from(value: User) -> Self {
        Self {
            nickname: value.nickname,
            gender: value.gender,
        }
    }
}
```