use std::collections::HashMap;
use redis::{cmd, Value};
use redis_test::{MockCmd, MockRedisConnection};
use serde::{Deserialize, Serialize};
use tokio::test;
use dce_session::redis::RedisBasic;
use dce_session::session::{Meta, Session};
use crate::common::redis;

mod common;

#[test]
async fn basic() {
    let sid_pool = (0..1).map(|_| Meta::gen_id(60).unwrap().0).collect::<Vec<_>>();
    let skey = RedisBasic::<MockRedisConnection>::gen_key("dcesid", &sid_pool[0]);
    let redis = redis(vec![
        MockCmd::new(cmd("HSET").arg(skey.as_str()).arg("uid").arg("10000"), Ok("1")),
        MockCmd::new(cmd("EXPIRE").arg(skey.as_str()).arg("3600"), Ok("1")),
        MockCmd::new(cmd("HGET").arg(skey.as_str()).arg("uid"), Ok("10000")),
        MockCmd::new(cmd("EXPIRE").arg(skey.as_str()).arg("3600"), Ok("true")),
        MockCmd::new(cmd("DEL").arg(skey.as_str()), Ok("1")),
        MockCmd::new(cmd("DEL").arg(skey.as_str()), Ok("0")),
    ]);

    let mut session = RedisBasic::new_with_id(sid_pool.clone()).unwrap().with(redis);
    session.set("uid", &10000).await.unwrap();
    let id = session.id().to_string();
    let uid: u64 = session.get("uid").await.unwrap();
    let result = session.destroy().await.unwrap();
    let result2 = session.destroy().await.unwrap();

    assert_eq!(id, sid_pool[0]);
    assert_eq!(uid, 10000);
    assert_eq!(result, true);
    assert_eq!(result2, false);
}

#[test]
async fn struct_store() {
    let sid_pool = (0..1).map(|_| Meta::gen_id(60).unwrap().0).collect::<Vec<_>>();
    let skey = RedisBasic::<MockRedisConnection>::gen_key("dcesid", &sid_pool[0]);
    let user = User {
        id: 10000,
        nick: "Dce".to_string(),
        gender: Gender::Male,
    };
    let user_string = serde_json::to_string::<User>(&user).unwrap();
    let items = vec!["user", &user_string];
    let items_value = Value::Bulk(items.iter().map(|e| Value::Data(e.as_bytes().into())).collect());
    let map = items.chunks(2).map(|cks| (cks[0].to_string(), cks[1].to_string())).collect::<HashMap<_, _>>();
    let redis = redis(vec![
        MockCmd::new(cmd("HSET").arg(skey.as_str()).arg("user").arg(user_string.as_str()), Ok("1")),
        MockCmd::new(cmd("EXPIRE").arg(skey.as_str()).arg("3600"), Ok("1")),
        MockCmd::new(cmd("HGET").arg(skey.as_str()).arg("user"), Ok(user_string)),
        MockCmd::new(cmd("HGETALL").arg(skey.as_str()), Ok(items_value)),
    ]);

    let mut session = RedisBasic::new_with_id(sid_pool).unwrap().with(redis);
    session.set("user", &user).await.unwrap();
    let u: User = session.get("user").await.unwrap();
    let data = session.raw().await.unwrap();

    assert_eq!(u, user);
    assert_eq!(data, map);
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct User {
    id: u64,
    nick: String,
    gender: Gender,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum Gender {
    Male,
    Female,
}
