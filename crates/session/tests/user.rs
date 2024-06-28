use std::collections::BTreeSet;
use redis::{cmd, Value};
use redis_test::{MockCmd, MockRedisConnection};
use serde::{Deserialize, Serialize};
use tokio::test;
use dce_session::redis::RedisSession;
use dce_session::session::{Meta, Session};
use dce_session::user::{UidGetter, User};
use crate::common::redis;

mod common;

#[test]
async fn login() {
    let (sid_pool, commands, user) = commends();
    let redis = redis(commands);

    let mut session = RedisSession::<_, Member>::new_with_id(sid_pool).unwrap().with(redis);
    session.login(user.clone(), 10080).await.unwrap();
    let u = session.user().await.unwrap();

    assert_eq!(u, &user);
    assert_eq!(u.id, user.id);
    assert_eq!(u.id.clone(), session.uid().await.unwrap());
}

#[test]
async fn sync() {
    let user_new = Member{
        id: 10000,
        nick: "Drunk".to_string(),
        gender: Gender::Male,
        role_id: 100,
    };
    let user_new_string = serde_json::to_string::<Member>(&user_new).unwrap();
    let (sid_pool1, mut commands1, user1) = commends();
    commands1.push(MockCmd::new(cmd("HGET").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &sid_pool1[1])).arg("$user"), Ok(user_new_string.clone())));
    let redis1 = redis(commands1);
    let (sid_pool2, mut commands2, user2) = commends();
    commands2.push(MockCmd::new(cmd("HGET").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &sid_pool2[1])).arg("$user"), Ok(user_new_string.clone())));
    let redis2 = redis(commands2);

    let mut session1 = RedisSession::<_, Member>::new_with_id(sid_pool1.clone()).unwrap().with(redis1);
    session1.login(user1.clone(), 10080).await.unwrap();
    let mut session2 = RedisSession::<_, Member>::new_with_id(sid_pool2.clone()).unwrap().with(redis2);
    session2.login(user1.clone(), 10080).await.unwrap();
    let u1 = session1.user().await.unwrap().clone();
    let u2 = session2.user().await.unwrap().clone();

    let umapped_sids = [&sid_pool1[1], &sid_pool2[1]].into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
    let usids_value = umapped_sids.iter().map(|id| Value::Data(id.as_bytes().into())).collect::<Vec<_>>();
    let ukey = RedisSession::<MockRedisConnection, Member>::gen_user_key("dceusmap", u1.id);
    let manager_sid_pool = (0..1).map(|_| Meta::gen_id(60).unwrap().0).collect::<Vec<_>>();
    let manager_redis = redis(vec![
        MockCmd::new(cmd("SMEMBERS").arg(ukey), Ok(Value::Bulk(usids_value))),
        MockCmd::new(cmd("EXISTS").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &umapped_sids[0])), Ok("1")),
        MockCmd::new(cmd("EXISTS").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &umapped_sids[1])), Ok("1")),
        MockCmd::new(cmd("HSET").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &umapped_sids[0])).arg("$user").arg(&user_new_string), Ok("1")),
        MockCmd::new(cmd("HSET").arg(RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &umapped_sids[1])).arg("$user").arg(&user_new_string), Ok("1")),
    ]);

    let mut manager_session = RedisSession::<_, Member>::new_with_id(manager_sid_pool).unwrap().with(manager_redis);
    manager_session.sync(&user_new).await.unwrap();

    let u1_new = session1.get::<Member>("$user").await.unwrap();
    let u2_new = session2.get::<Member>("$user").await.unwrap();

    assert_eq!(u1, u2);
    assert_eq!(u1_new, u2_new);
    assert_eq!(u1, user2);
    assert_ne!(u1, u1_new);
    assert_eq!(u1.role_id, 1);
    assert_eq!(u1_new.role_id, 100);
}

fn commends() -> (Vec<String>, Vec<MockCmd>, Member) {
    let sid_pool = (0..2).map(|_| Meta::gen_id(60).unwrap().0).collect::<Vec<_>>();
    let skey = RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &sid_pool[0]);
    let skey2 = RedisSession::<MockRedisConnection, Member>::gen_key("dcesid", &sid_pool[1]);
    let user = Member {
        id: 10000,
        nick: "Dce".to_string(),
        gender: Gender::Male,
        role_id: 1,
    };
    let ukey = RedisSession::<MockRedisConnection, Member>::gen_user_key("dceusmap", user.id);
    let user_string = serde_json::to_string::<Member>(&user).unwrap();
    let commands = vec![
        MockCmd::new(cmd("HGETALL").arg(skey.as_str()), Ok(Value::Bulk(vec![]))),
        MockCmd::new(cmd("HMSET").arg(skey2.as_str()).arg("$user").arg(user_string.as_str()), Ok("1")),
        MockCmd::new(cmd("EXPIRE").arg(skey2.as_str()).arg("604800"), Ok("1")),
        MockCmd::new(cmd("HGET").arg(skey2.as_str()).arg("$user"), Ok(user_string)),
        MockCmd::new(cmd("SADD").arg(ukey.as_str()).arg(&sid_pool[1]), Ok("1")),
        MockCmd::new(cmd("EXPIRE").arg(ukey.as_str()).arg("604800"), Ok("1")),
        MockCmd::new(cmd("DEL").arg(skey.as_str()), Ok("1")),
    ];
    (sid_pool, commands, user)
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct Member {
    id: u64,
    nick: String,
    gender: Gender,
    role_id: u16,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
enum Gender {
    Male,
    Female,
}

impl UidGetter for Member {
    fn id(&self) -> u64 {
        self.id
    }
}