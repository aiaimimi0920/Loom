# Loom Dependency Graph

```mermaid
flowchart TD
    subgraph P0[Phase 0: Audit]
        T01[T0.1 Source migration matrix]
        T02[T0.2 ArtLoom old/new delta compare]
        T03[T0.3 Lock v1 scope]
        T01 --> T03
        T02 --> T03
    end

    subgraph P1[Phase 1: Workspace]
        T11[T1.1 Loom Rust workspace]
        T12[T1.2 README and architecture docs]
        T11 --> T12
    end

    subgraph P2[Phase 2: Core and Durable]
        T21[T2.1 Core primitives]
        T22[T2.2 In-memory event store]
        T23[T2.3 Actor mesh]
        T21 --> T22
        T21 --> T23
    end

    subgraph P3[Phase 3: Agents and Workflows]
        T31[T3.1 Agent definition loader]
        T32[T3.2 Workflow graph model]
        T33[T3.3 Workflow executor]
        T34[T3.4 Cognitive orchestration facade]
        T32 --> T33
        T22 --> T33
        T23 --> T33
        T31 --> T34
        T33 --> T34
    end

    subgraph P4[Phase 4: Gateway, Sandbox, Hooks]
        T41[T4.1 Gateway client]
        T42[T4.2 Sandbox contract]
        T43[T4.3 Hooks]
    end

    subgraph P5[Phase 5: Daemon and CLI]
        T51[T5.1 Daemon runtime]
        T52[T5.2 CLI]
        T53[T5.3 Fixtures]
        T51 --> T52
        T31 --> T53
        T33 --> T53
    end

    subgraph P6[Phase 6: ArtLoom Adapters]
        T61[T6.1 ArtLoom workflow converter]
        T62[T6.2 ArtLoom smoke patterns]
        T61 --> T62
    end

    subgraph P7[Phase 7: Baseline]
        T71[T7.1 Full validation]
        T72[T7.2 Migration baseline docs]
        T71 --> T72
    end

    T03 --> T11
    T11 --> T21
    T21 --> T31
    T21 --> T32
    T21 --> T41
    T21 --> T42
    T21 --> T43
    T31 --> T51
    T33 --> T51
    T41 --> T51
    T42 --> T51
    T43 --> T51
    T32 --> T61
    T52 --> T62
    T53 --> T62
    T52 --> T71
    T62 --> T71
```

## Execution order

1. Finish Phase 0 audit before creating code.
2. Build workspace skeleton.
3. Implement core primitives before durable/agent/workflow modules.
4. Implement daemon only after agent, workflow, Gateway, sandbox, and hooks have
   testable contracts.
5. Add ArtLoom converters only after native Loom workflow contracts are stable.
6. Run full validation before declaring a Loom baseline.
