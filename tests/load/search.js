import http from "k6/http";
import { check, sleep } from "k6";

const baseUrl = __ENV.SEARXFLARE_BASE_URL;
const apiKey = __ENV.SEARXFLARE_API_KEY;

if (!baseUrl || !apiKey) {
  throw new Error("SEARXFLARE_BASE_URL and SEARXFLARE_API_KEY are required");
}

export const options = {
  scenarios: {
    warm_search: {
      executor: "constant-arrival-rate",
      rate: 20,
      timeUnit: "1s",
      duration: "2m",
      preAllocatedVUs: 20,
      maxVUs: 100,
    },
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    http_req_duration: ["p(95)<5000"],
    checks: ["rate>0.99"],
  },
};

export default function () {
  const query = encodeURIComponent("cloudflare rust workers");
  const response = http.get(`${baseUrl}/v1/search?q=${query}`, {
    headers: { Authorization: `Bearer ${apiKey}` },
    tags: { route: "v1-search" },
  });
  check(response, {
    "search succeeds": (result) => result.status === 200,
    "response is bounded": (result) => result.body.length < 2 * 1024 * 1024,
    "request id is present": (result) => Boolean(result.headers["X-Request-Id"]),
  });
  sleep(0.1);
}
