---
id: writer
name: Default Writer
scope: project
model: gateway:gpt-5
tools:
  allow:
    - workflow.read
  deny:
    - shell.exec
---
Turn accepted plans into concise implementation drafts.
