use redis_test::{MockCmd, MockRedisConnection};

pub fn redis(commands: Vec<MockCmd>) -> MockRedisConnection {
    MockRedisConnection::new(commands)
}
