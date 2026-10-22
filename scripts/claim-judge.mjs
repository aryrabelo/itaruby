#!/usr/bin/env node
// The oracle L4.CLAIM_IS_SUPPORTED_BY_ITS_EVIDENCE runs.
//
// Exit codes are the whole contract:
//   0  expected question, expected choice, confidence at or above the floor
//   1  the finding — wrong choice, or the right choice below the floor
//   2  usage, request, cache or API failure; also a finding, never a skip

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

const die = (code, message) => {
  console.error(message);
  process.exit(code);
};

// --- the command line -------------------------------------------------------

const flag = (name) => {
  const at = process.argv.indexOf(`--${name}`);
  return at >= 0 ? process.argv[at + 1] : undefined;
};

const requestPath = flag("request");
const cachePath = flag("cache");
const expected = flag("expect");
const floor = Number(flag("min-confidence") ?? 0.8);

if (!requestPath || !cachePath || !expected)
  die(2, "usage: claim-judge.mjs --request FILE --expect name=label --min-confidence N --cache FILE");

const [question, wanted] = expected.split("=");
if (!question || !wanted)
  die(2, `--expect must be name=label, got "${expected}"`);

// --- the request ------------------------------------------------------------

let request;
try {
  request = JSON.parse(readFileSync(requestPath, "utf8"));
} catch (error) {
  die(2, `cannot read ${requestPath}: ${error.message}`);
}

if (!request.state?.claim || !request.questions?.[question])
  die(2, `${requestPath} needs state.claim and a question named "${question}"`);

// --- the cache: everything that changes the verdict is the key --------------

const model = request.model ?? "jev-latest";
const key = createHash("sha256")
  .update(JSON.stringify({ model, state: request.state, questions: request.questions }))
  .digest("hex");

let cache = {};
if (existsSync(cachePath)) {
  try {
    cache = JSON.parse(readFileSync(cachePath, "utf8"));
  } catch (error) {
    die(2, `cannot read ${cachePath}: ${error.message}`);
  }
}

// --- ask, unless this exact question was already answered -------------------

let answer = cache[key];
if (!answer) {
  const apiKey = process.env.TYPESAFE_API_KEY;
  if (!apiKey)
    die(2, "TYPESAFE_API_KEY is not set — an oracle nobody can run is itself the finding");

  const base = process.env.TYPESAFE_BASE_URL ?? "https://api.typesafe.ai";
  let response;
  try {
    response = await fetch(`${base}/v1/systemone`, {
      method: "POST",
      headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
      body: JSON.stringify({ model, state: request.state, questions: request.questions }),
    });
  } catch (error) {
    die(2, `cannot reach ${base}: ${error.message}`);
  }
  if (!response.ok)
    die(2, `the API answered ${response.status}: ${(await response.text()).slice(0, 400)}`);

  const judged = await response.json();
  answer = judged.answers?.[question];
  if (!answer?.choice)
    die(2, `the API returned no choice for "${question}"`);

  cache[key] = answer;
  try {
    writeFileSync(cachePath, `${JSON.stringify(cache, null, 2)}\n`);
  } catch (error) {
    die(2, `cannot write ${cachePath}: ${error.message}`);
  }
}

// --- the verdict ------------------------------------------------------------

const confidence = answer.confidence ?? 0;
const claim = request.state.claim;

if (answer.choice !== wanted) {
  console.error(`claim not supported: "${claim}"`);
  console.error(`judged ${answer.choice} at ${confidence.toFixed(2)}, expected ${wanted}`);
  die(1, "the evidence does not support the sentence");
}
if (confidence < floor) {
  console.error(`claim supported without conviction: "${claim}"`);
  console.error(`${wanted} at ${confidence.toFixed(2)}, below the floor ${floor}`);
  die(1, "right choice, no conviction — usually a badly posed question or thin evidence");
}

console.log(`claim supported: "${claim}" — ${wanted} at ${confidence.toFixed(2)} (floor ${floor})`);
