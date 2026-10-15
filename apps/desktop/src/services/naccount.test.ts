import assert from "node:assert/strict";
import test from "node:test";
import { cloudError, parseNAccount } from "./naccount.ts";

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
