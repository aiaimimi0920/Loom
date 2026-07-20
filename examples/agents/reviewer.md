---
id: reviewer
name: Default Reviewer
scope: project
model: gateway:gpt-5
tools:
  allow:
    - workflow.read
  deny:
    - shell.exec
---
Review Loom workflow outputs for correctness before completion.
