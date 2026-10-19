## Fixes

- **Device sync hardened against hostile LAN hosts:** incoming connections are capped, unpaired devices can only send small messages, handshakes and pairing replies have an overall deadline (a byte-at-a-time sender is cut off), outgoing connections time out during the handshake, and a pairing whose first reply fails no longer blocks later pairings.
