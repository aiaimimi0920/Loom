import assert from "node:assert/strict";
import test from "node:test";
import { cloudError, naccountRequest, parseNAccount } from "./naccount.ts";

test("NAccount 视图保持真实配置状态且不向 UI 返回令牌", () => {
  assert.deepEqual(parseNAccount({ status: "signed_out", configured: false }),
    { status: "signed_out", configured: false, remoteRevoked: undefined });
  assert.deepEqual(parseNAccount({ status: "signed_in", principal: "user", name: "用户", access_token: "secret" }),
    { status: "signed_in", principal: "user", name: "用户" });
  assert.throws(() => parseNAccount({ status: "signed_in", principal: "", name: "用户" }));
});
test("NAccount 浏览器登录链接匹配本次请求，不接受旧 Platform 协议", () => {
  const requestId = "a".repeat(43);
  const authorizationUrl = `https://naccount-auth.aiaimimi.com/oauth2/v1/authorize?state=${requestId}`;
  assert.equal(parseNAccount({ status: "pending", requestId, authorizationUrl }).status, "pending");
  for (const url of [authorizationUrl.replace("/oauth2/v1/authorize", "/loom/authorize"),
    `${authorizationUrl}&state=other`, authorizationUrl.replace("naccount-auth.aiaimimi.com", "other.test")])
    assert.throws(() => parseNAccount({ status: "pending", requestId, authorizationUrl: url }));
  assert.match(cloudError(new Error("asset_library_distribution_unavailable")), /暂未开放/);
});

test("NAccount 区分桌面调用失败与未配置，错误不泄露请求内容", () => {
  for (const [message, expected] of [
    ["/v1/naccount/status returned HTTP/1.1 401 Unauthorized", "身份校验失败"],
    ["invalid args for command: missing required key baseUrl", "请求参数无效"],
    ["Command post_loom_daemon_json not found", "登录接口不可用"],
    ["Failed to fetch", "无法连接本机"],
    ["naccount_response_invalid", "登录状态无效"],
  ]) assert.ok(cloudError(new Error(message)).includes(expected));
  assert.equal(cloudError(new Error("unknown sensitive-value")), "请求未完成，请重试连接。");
});

test("原生状态请求使用 Tauri 桥且失败后可重新读取真实配置", async (context) => {
  const names = ["window", "isTauri", "__TAURI_INTERNALS__"];
  const previous = names.map((name) => [name, Reflect.has(globalThis, name), Reflect.get(globalThis, name)] as const);
  const fetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = fetch;
    for (const [name, present, value] of previous) {
      if (present) Reflect.set(globalThis, name, value);
      else Reflect.deleteProperty(globalThis, name);
    }
  });
  let calls = 0;
  Reflect.set(globalThis, "window", globalThis);
  Reflect.set(globalThis, "isTauri", true);
  globalThis.fetch = async () => { throw new Error("原生请求不得回退到未认证 fetch"); };
  Reflect.set(globalThis, "__TAURI_INTERNALS__", {
    invoke: async (command: string, args: unknown) => {
      assert.equal(command, "post_loom_daemon_json");
      assert.deepEqual(args, { baseUrl: "http://127.0.0.1:8765", path: "/v1/naccount/status", body: {} });
      if (++calls === 1) throw "HTTP 401 Unauthorized";
      return { status: "signed_out", configured: true };
    },
  });
  await assert.rejects(naccountRequest("http://127.0.0.1:8765", "status"), /401/);
  const view = await naccountRequest("http://127.0.0.1:8765", "status");
  assert.deepEqual(view, { status: "signed_out", configured: true, remoteRevoked: undefined });
  assert.equal(calls, 2);
});
