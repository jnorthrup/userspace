//! Channel exports

pub mod channel;

pub use channel::{
    buffered_channel, channel, unbounded_channel, BufferedChannel, Channel,
    ChannelCapacity, RecvError, RecvFuture, RendezvousChannel, SendError, SendFuture,
    UnboundedChannel, Receiver, Sender,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_creation() {
        let (tx, rx) = channel::<i32>();
        tx.send(1).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_buffered_channel() {
        let (tx, rx) = buffered_channel::<i32>(10);
        for i in 0..10 {
            tx.send(i).await.unwrap();
        }
        for i in 0..10 {
            assert_eq!(rx.recv().await.unwrap(), i);
        }
    }

    #[tokio::test]
    async fn test_unbounded_channel() {
        let (tx, rx) = unbounded_channel::<i32>();
        for i in 0..100 {
            tx.send(i).await.unwrap();
        }
        for i in 0..100 {
            assert_eq!(rx.recv().await.unwrap(), i);
        }
    }

    #[tokio::test]
    async fn test_channel_capacity_traits() {
        let (tx, _) = channel::<i32>();
        assert_eq!(tx.capacity(), ChannelCapacity::Rendezvous);

        let (tx, _) = buffered_channel::<i32>(5);
        assert_eq!(tx.capacity(), ChannelCapacity::Buffered(5));

        let (tx, _) = unbounded_channel::<i32>();
        assert_eq!(tx.capacity(), ChannelCapacity::Unbounded);
    }
}
