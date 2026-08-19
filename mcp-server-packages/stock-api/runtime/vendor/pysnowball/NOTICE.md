# pysnowball integration notice

The `stock-api` MCP wrapper includes a Node.js adapter compatible with the
REST endpoints and authentication behavior documented by pysnowball 0.1.8:

- Upstream: https://github.com/uname-yang/pysnowball
- Revision: `e85fe550c5daed4ad1429d1f4e048dab239df921`
- License: Apache-2.0

The Python package source and a Python runtime are not bundled or executed.
The adapter uses the anonymous `quotec` endpoint for realtime snapshots and,
when `LOOM_PYSNOWBALL_TOKEN` is configured, the credentialed `pankou`
endpoint for order-book depth. The existing Xueqiu-compatible request path
remains available as a fallback.

`LOOM_PYSNOWBALL_TOKEN` is treated as a secret Cookie value and is never
included in MCP responses or logs.
