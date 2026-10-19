## Fixes

- **Idle control connections no longer lock out MCP and the CLI:** a control-channel connection must authenticate within 10 s, an authenticated one idle for 15 min is closed (clients reconnect), and unauthenticated connections have their own cap so they can't take the slots authenticated clients need.
