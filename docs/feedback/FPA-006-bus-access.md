# FPA-006: Bus Access — Spec Implication

The spec should clarify that bus access is an implementation concern, not
a trait concern. The `Partition` trait intentionally does not include a
`set_bus()` method — this preserves strategy neutrality and avoids
coupling all partitions to the bus abstraction. Partitions that need bus
access accept an `Arc<dyn Bus>` in their constructor; partitions that
don't are unaffected.
