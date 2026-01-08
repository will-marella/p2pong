// libp2p NetworkBehaviour for P2Pong
// Handles custom protocol for game message streaming

use libp2p::{
    core::upgrade::{read_length_prefixed, write_length_prefixed},
    swarm::{
        ConnectionHandler, ConnectionHandlerEvent, KeepAlive, SubstreamProtocol,
        handler::ConnectionEvent, NetworkBehaviour, ToSwarm,
    },
    PeerId, StreamProtocol,
};
use futures::{AsyncReadExt, AsyncWriteExt};
use std::collections::VecDeque;
use void::Void;

use super::protocol::{NetworkMessage, PROTOCOL_ID};

/// Custom network behaviour for P2Pong game protocol
#[derive(Default)]
pub struct PongBehaviour {
    /// Queue of events to send to Swarm
    events: VecDeque<ToSwarm<PongEvent, Void>>,
}

/// Events emitted by PongBehaviour to the Swarm
#[derive(Debug)]
pub enum PongEvent {
    /// Received a message from a peer
    Message { peer: PeerId, message: NetworkMessage },
    
    /// Connection established with peer
    Connected { peer: PeerId },
    
    /// Connection lost with peer
    Disconnected { peer: PeerId },
}

impl PongBehaviour {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }
    
    /// Send a message to a specific peer
    /// TODO: Will be implemented when we add connection tracking
    pub fn send_message(&mut self, _peer: PeerId, _message: NetworkMessage) {
        // For now, we'll implement a simpler approach in the swarm event loop
        todo!("Message sending will be implemented in network thread")
    }
}

impl NetworkBehaviour for PongBehaviour {
    type ConnectionHandler = PongHandler;
    type ToSwarm = PongEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        _local_addr: &libp2p::Multiaddr,
        _remote_addr: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(PongHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        _addr: &libp2p::Multiaddr,
        _role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(PongHandler::new())
    }

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
        match event {
            libp2p::swarm::FromSwarm::ConnectionEstablished(e) => {
                self.events.push_back(ToSwarm::GenerateEvent(PongEvent::Connected {
                    peer: e.peer_id,
                }));
            }
            libp2p::swarm::FromSwarm::ConnectionClosed(e) => {
                self.events.push_back(ToSwarm::GenerateEvent(PongEvent::Disconnected {
                    peer: e.peer_id,
                }));
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        match event {
            HandlerEvent::Message(message) => {
                self.events.push_back(ToSwarm::GenerateEvent(PongEvent::Message {
                    peer: peer_id,
                    message,
                }));
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            std::task::Poll::Ready(event)
        } else {
            std::task::Poll::Pending
        }
    }
}

/// Connection handler for P2Pong protocol
pub struct PongHandler {
    /// Keep connection alive
    keep_alive: KeepAlive,
}

impl PongHandler {
    fn new() -> Self {
        Self {
            keep_alive: KeepAlive::Yes,
        }
    }
}

/// Events from handler to behaviour
#[derive(Debug)]
pub enum HandlerEvent {
    Message(NetworkMessage),
}

impl ConnectionHandler for PongHandler {
    type FromBehaviour = Void;
    type ToBehaviour = HandlerEvent;
    type InboundProtocol = StreamProtocol;
    type OutboundProtocol = StreamProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(StreamProtocol::new(PROTOCOL_ID), ())
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>> {
        std::task::Poll::Pending
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {
        void::unreachable(_event)
    }

    fn on_connection_event(
        &mut self,
        _event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol, Self::InboundOpenInfo, Self::OutboundOpenInfo>,
    ) {
        // Handle inbound/outbound stream events here
        // For now, simplified - will implement message handling in Day 3
    }
}
