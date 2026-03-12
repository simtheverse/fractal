# FPA-004: Network Transport Serialization

**Requirement:** FPA-004 specifies network transport with serialization.

**Issue:** The `Message` trait requires `Clone + Send + 'static + Any` but NOT
`Serialize + Deserialize`. Adding serde bounds to Message would require all message
types to derive serde traits, which may not be desirable for in-process-only messages.

**Finding:** NetworkBus is implemented as a structural stub (same as InProcessBus)
because real network serialization requires either:
1. Adding `Serialize + Deserialize` bounds to Message trait (breaking change)
2. A type-erased serialization layer (e.g., messages serialize to bytes)
3. A separate `NetworkMessage: Message + Serialize` trait

**Proposed Resolution:** Option 3 — introduce a `NetworkMessage` subtrait that adds
serde bounds. NetworkBus requires `NetworkMessage` instead of `Message`. This keeps
the base Message trait clean while enabling network transport for types that opt in.
