# FPA-004 Implicit Requirement: Framework Type Serialization

## Finding

When NetworkBus uses real serialization (codec-based), all framework message
types that cross the bus must implement `Serialize + Deserialize`. This includes:

- `SharedContext` (published every tick by the compositor)
- `TransitionRequest` (state machine transitions)
- `DumpRequest` / `LoadRequest` (dump/load lifecycle)
- `ExecutionState` (embedded in SharedContext and TransitionRequest)

The base `Message` trait intentionally omits serde bounds (keeping it clean for
in-process use), but network transport makes serialization a hard requirement
for these specific types.

## Implication

This is an implicit requirement not stated in FPA-004. The prototype resolves
it by:

1. Adding `Serialize + Deserialize` derives to all framework message types
2. Introducing a `NetworkMessage` subtrait (`Message + Serialize + DeserializeOwned`)
3. Requiring codec registration per type on `NetworkBus`
4. Providing `with_framework_codecs()` to pre-register all framework types

The design keeps the base `Message` trait clean while making the serialization
requirement explicit and opt-in at the transport level.

## Recommendation

Make explicit in the spec that framework message types must be serializable
when network transport is in use. This could be a note under FPA-004 or a
new sub-requirement (e.g., FPA-004a).
