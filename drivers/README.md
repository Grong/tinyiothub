# Dynamic drivers

`demo-declarative/` is the reference driver and the template for new dynamic
drivers. Copy it, rename the crate/manifest identity, and implement the ABI v2
vtable (`tinyiothub_driver_vtable_v2`, async read/execute + cancel + describe).

The former protocol placeholders (`bacnet`, `modbus`, `mqtt`, `mqtt-bridge`,
`onvif`, `opcua`, `snmp`) were empty stubs (a bare `driver_init()` or a module
comment) with no manifest or ABI v2 exports. They were retired on 2026-10-23
rather than migrated as fake drivers. Real protocol drivers are future work;
see `docs/superpowers/plans/2026-10-23-e4e-migration-plan.md` §3 for the
proposed order and per-package acceptance gate.

Native built-ins (`simulator`, `modbus_rtu`, `snmp`) live in
`crates/runtime/src/driver/drivers/` with manifests in
`crates/runtime/manifests/`.
