use std::thread;
use std::time::Duration;
use tipc::{GroupMessage, SockType, TipcConn, TipcResult};

const SERVER_ADDR: u32 = 12345u32;
const SERVER_INST: u32 = 0u32;
const SERVER_NODE: u32 = 0u32;
const SERVER_SCOPE: tipc::TipcScope = tipc::TipcScope::Cluster;

const CLIENT_ADDR: u32 = 12345;
const CLIENT_INST: u32 = 100;
const CLIENT_SCOPE: tipc::TipcScope = tipc::TipcScope::Cluster;

static TEST_MESSAGE: &str = "Test Message 4242! The quick brown fox jumped over the lazy dog.";

#[test]
fn test_set_non_blocking_returns_error()
{
    let mut conn = TipcConn::new(SockType::SockStream).unwrap();
    conn.set_sock_non_block().unwrap();

    conn.bind(SERVER_ADDR, SERVER_INST, SERVER_NODE, SERVER_SCOPE).unwrap();
    conn.listen(1).unwrap();
    let e = conn.accept().unwrap_err();
    assert_eq!(e.code(), 11);
}

#[test]
fn test_anycast()
{
    let mut server1 = setup_listen_server(SockType::SockRdm);
    let mut server2 = setup_listen_server(SockType::SockRdm);

    server1.set_sock_non_block().unwrap();
    server2.set_sock_non_block().unwrap();

    let client = TipcConn::new(SockType::SockRdm).unwrap();

    let mut pri = &server1;
    let mut sec = &server2;

    thread::sleep(Duration::from_millis(10));

    for _ in 1..10
    {
        let bytes_sent = client
            .anycast(
                TEST_MESSAGE.as_bytes(),
                SERVER_ADDR,
                SERVER_INST,
            )
            .unwrap();
        assert_eq!(bytes_sent as usize, TEST_MESSAGE.len());

        match assert_message_received(pri, TEST_MESSAGE)
        {
            Err(errno) => {
                assert_eq!(errno.code(), 11);
                assert_message_received(sec, TEST_MESSAGE).unwrap();
            },

            _ => {
                let tmp = pri;
                pri = sec;
                sec = tmp;
            },
        }
    }
}

#[test]
#[ignore]
fn test_broadcast()
{
    let mut server1 = TipcConn::new(SockType::SockRdm).unwrap();
    server1
        .join(SERVER_ADDR, SERVER_INST, SERVER_SCOPE)
        .unwrap();

    let mut server2 = TipcConn::new(SockType::SockRdm).unwrap();
    server2
        .join(SERVER_ADDR, SERVER_INST + 1, SERVER_SCOPE)
        .unwrap();

    // Both servers receive a message for each other's join
    assert_message_received(&server1, "").unwrap();
    assert_message_received(&server2, "").unwrap();

    let mut client = TipcConn::new(SockType::SockRdm).unwrap();
    client.join(CLIENT_ADDR, CLIENT_INST, CLIENT_SCOPE).unwrap();

    // Servers receive message for client joining
    assert_message_received(&server1, "").unwrap();
    assert_message_received(&server2, "").unwrap();

    let bytes_sent = client.broadcast(TEST_MESSAGE.as_bytes()).unwrap();
    assert_eq!(bytes_sent as usize, TEST_MESSAGE.len());

    assert_message_received(&server1, TEST_MESSAGE).unwrap();
    assert_message_received(&server2, TEST_MESSAGE).unwrap();
}

#[test]
fn test_multicast()
{
    let mut server1 = setup_listen_server(SockType::SockRdm);
    let mut server2 = setup_listen_server(SockType::SockRdm);

    server1.set_sock_non_block().unwrap();
    server2.set_sock_non_block().unwrap();

    let client = TipcConn::new(SockType::SockRdm).unwrap();
    let bytes_sent = client
        .multicast(
            TEST_MESSAGE.as_bytes(),
            SERVER_ADDR,
            SERVER_INST,
            SERVER_INST + 1,
            SERVER_SCOPE
        ).unwrap();
    assert_eq!(bytes_sent as usize, TEST_MESSAGE.len());

    assert_message_received(&server1, TEST_MESSAGE).unwrap();
    assert_message_received(&server2, TEST_MESSAGE).unwrap();
}

#[test]
fn test_unicast()
{
    let server = setup_listen_server(SockType::SockRdm);

    let client = TipcConn::new(SockType::SockRdm).unwrap();
    let bytes_sent = client
        .unicast(
            TEST_MESSAGE.as_bytes(),
            server.socket_ref(),
            server.node_ref(),
        ).unwrap();

    assert_eq!(bytes_sent as usize, TEST_MESSAGE.len());

    assert_message_received(&server, TEST_MESSAGE).unwrap();
}

#[test]
fn test_connect_and_send()
{
    let server = setup_listen_server(SockType::SockSeqpacket);
    server.listen(1).unwrap();

    let t1 = thread::spawn(move || {
        let new_conn = server.accept().unwrap();
        assert_message_received(&new_conn, TEST_MESSAGE).unwrap();
    });

    let client = TipcConn::new(SockType::SockSeqpacket).unwrap();
    match client.connect(SERVER_ADDR, SERVER_INST, SERVER_NODE)
    {
        Ok(_) => {
            let bytes_sent = client.send(TEST_MESSAGE.as_bytes()).unwrap();
            assert_eq!(bytes_sent as usize, TEST_MESSAGE.len());
        },

        Err(_) => {
            client.connect(SERVER_ADDR, SERVER_INST, SERVER_NODE).unwrap();
        },
    };

    t1.join().expect("Couldn't join on the associated thread")
}

#[test]
fn test_join_and_leave_membership_event()
{
    let mut server = TipcConn::new(SockType::SockRdm).unwrap();
    server.join(SERVER_ADDR, SERVER_INST, SERVER_SCOPE).unwrap();

    let t1 = thread::spawn(move || {
        // First message will be a join
        match server.recvfrom().unwrap() {
            GroupMessage::MemberEvent(e) => {
                assert!(e.joined());
                assert_eq!(e.service_type(), CLIENT_ADDR);
                assert_eq!(e.service_instance(), CLIENT_INST);
            }
            _ => panic!("Unexpected data group data message"),
        }

        // Second message will be a leave
        match server.recvfrom().unwrap() {
            GroupMessage::MemberEvent(e) => {
                assert!(!e.joined());
                assert_eq!(e.service_type(), CLIENT_ADDR);
                assert_eq!(e.service_instance(), CLIENT_INST);
            }
            _ => panic!("Unexpected data group data message"),
        }
    });

    let mut client = TipcConn::new(SockType::SockRdm).unwrap();
    client.join(CLIENT_ADDR, CLIENT_INST, CLIENT_SCOPE).unwrap();
    thread::sleep(Duration::from_millis(100));
    client.leave().unwrap();

    t1.join().expect("Couldn't join on the associated thread")
}

fn setup_listen_server(socktype: SockType) -> TipcConn
{
    let server = TipcConn::new(socktype).unwrap();
    server.bind(SERVER_ADDR, SERVER_INST, SERVER_NODE, SERVER_SCOPE).unwrap();
    server
}

fn assert_message_received(conn: &TipcConn, expected_msg: &str) -> TipcResult<i32>
{
    let mut buf: [u8; tipc::MAX_MSG_SIZE] = [0; tipc::MAX_MSG_SIZE];
    let res = conn.recv(&mut buf);
    
    match res
    {
        Ok(msg_size) => {
            assert_eq!(std::str::from_utf8(&buf[0..msg_size as usize]).unwrap(), expected_msg);
            
        },
        _ => {},
    }

    res
}
