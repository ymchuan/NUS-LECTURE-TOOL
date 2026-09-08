#!/usr/bin/env node

import { writeFile } from "node:fs/promises";
import process from "node:process";
import { performance } from "node:perf_hooks";

const DEFAULT_ENDPOINT = "http://127.0.0.1:11434/v1";
const DEFAULT_MODEL = "translategemma:4b";
const DEFAULT_RUNS = 1;
const DEFAULT_TIMEOUT_MS = 90_000;
const DEFAULT_MODE = "stream";

const SAMPLES = [
  {
    id: "ml-gradient-negation",
    course: "CEG5301 Machine Learning with Applications",
    source: "The gradient does not converge when the learning rate exceeds 0.1.",
    glossary: ["gradient → 梯度", "learning rate → 学习率"],
    terms: [["梯度"], ["学习率"]],
    numbers: [["0.1"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "cellular-handover-threshold",
    course: "CEG5104 Cellular Networks",
    source: "A handover is not triggered unless RSRP stays below -110 dBm for 3 consecutive measurement periods.",
    glossary: ["handover → 切换", "RSRP → 保留英文", "dBm → 保留英文"],
    terms: [["切换"], ["RSRP"], ["dBm"]],
    numbers: [["-110", "−110"], ["3", "三"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "queueing-littles-law",
    course: "CEG5104 Cellular Networks",
    source: "Little's Law says L = λW, but it does not assume Poisson arrivals.",
    glossary: ["Little's Law → 利特尔定律", "Poisson arrival → 泊松到达"],
    terms: [["利特尔定律"], ["泊松"]],
    numbers: [],
    formulas: [["L=λW", "L=lambdaW"]],
    requiresNegation: true,
  },
  {
    id: "economics-elasticity",
    course: "Microeconomics",
    source: "A price elasticity of -1.5 does not mean demand rises when price rises by 10%.",
    glossary: ["price elasticity → 价格弹性", "demand → 需求"],
    terms: [["价格弹性"], ["需求"]],
    numbers: [["-1.5", "−1.5"], ["10%", "10％"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "classification-error-rates",
    course: "CEG5301 Machine Learning with Applications",
    source: "The false-positive rate fell from 2.4% to 0.8%; we cannot ignore false negatives.",
    glossary: ["false positive → 假阳性", "false negative → 假阴性"],
    terms: [["假阳性", "误报"], ["假阴性", "漏报"]],
    numbers: [["2.4%", "2.4％"], ["0.8%", "0.8％"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "network-jitter-buffer",
    course: "Computer Networks",
    source: "With a 20 ms jitter buffer, packets that arrive out of order must not be silently discarded.",
    glossary: ["jitter buffer → 抖动缓冲区", "packet → 数据包"],
    terms: [["抖动缓冲"], ["数据包", "分组"]],
    numbers: [["20 ms", "20ms", "20 毫秒", "20毫秒"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "bayes-formula",
    course: "CEG5301 Machine Learning with Applications",
    source: "The posterior is proportional to likelihood times prior: p(θ|x) ∝ p(x|θ)p(θ).",
    glossary: ["posterior → 后验", "likelihood → 似然", "prior → 先验"],
    terms: [["后验"], ["似然"], ["先验"]],
    numbers: [],
    formulas: [["p(θ|x)∝p(x|θ)p(θ)"]],
    requiresNegation: false,
  },
  {
    id: "statistics-spoken-negation",
    course: "Applied Statistics",
    source: "So, uh, the confidence interval is 95%, which doesn't prove the null hypothesis is true.",
    glossary: ["confidence interval → 置信区间", "null hypothesis → 原假设"],
    terms: [["置信区间"], ["原假设", "零假设"]],
    numbers: [["95%", "95％"]],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "cellular-authentication-order",
    course: "CEG5104 Cellular Networks",
    source: "The UE may camp on LTE, but it must not attach to the network before authentication completes.",
    glossary: ["UE → 保留英文", "LTE → 保留英文", "authentication → 认证"],
    terms: [["UE"], ["LTE"], ["认证"]],
    numbers: [],
    formulas: [],
    requiresNegation: true,
  },
  {
    id: "numerical-rounding-boundary",
    course: "Numerical Methods",
    source: "If x ≤ 10^-3, do not round it to zero.",
    glossary: ["round to zero → 舍入为零"],
    terms: [["舍入", "取整"]],
    numbers: [],
    formulas: [["x≤10^-3", "x≤10⁻³", "x<=10^-3"]],
    requiresNegation: true,
  },
];

function usage() {
  return `Usage:
  node scripts/benchmark-local-translation.mjs [options]

Options:
  --endpoint <url>       OpenAI-compatible base URL (default: ${DEFAULT_ENDPOINT})
  --model <name>         Local model name (default: ${DEFAULT_MODEL})
  --runs <number>        Warm passes over all samples (default: ${DEFAULT_RUNS})
  --mode <value>         stream or non-stream (default: ${DEFAULT_MODE})
  --concurrency <value>  Warm concurrency plan: 1, 2, or 1,2 (default: 1)
  --include-usage        Ask streaming servers to include a usage trailer when supported
  --timeout-ms <number>  Timeout for each request (default: ${DEFAULT_TIMEOUT_MS})
  --output <path>        Also write the JSON result to this file
  --list-samples         Print the built-in sample IDs and exit
  --help                 Show this help

Only http://127.0.0.1, http://[::1], and http://localhost endpoints are accepted.`;
}

function optionValue(argv, index, name) {
  const argument = argv[index];
  const prefix = `${name}=`;
  if (argument.startsWith(prefix)) return { value: argument.slice(prefix.length), consumed: 0 };
  if (argument === name) {
    if (index + 1 >= argv.length) throw new Error(`${name} requires a value`);
    return { value: argv[index + 1], consumed: 1 };
  }
  return null;
}

function parseInteger(value, name, minimum, maximum) {
  if (!/^\d+$/.test(value)) throw new Error(`${name} must be an integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`${name} must be between ${minimum} and ${maximum}`);
  }
  return parsed;
}

function parseConcurrencyPlan(value) {
  const plan = value === "both" ? "1,2" : value;
  const values = plan.split(",").map((item) => item.trim()).filter(Boolean);
  if (values.length === 0 || values.some((item) => item !== "1" && item !== "2")) {
    throw new Error("--concurrency must be 1, 2, 1,2, or both");
  }
  return [...new Set(values.map(Number))];
}

function parseArguments(argv) {
  const config = {
    endpoint: process.env.LOCAL_TRANSLATION_ENDPOINT || DEFAULT_ENDPOINT,
    model: process.env.LOCAL_TRANSLATION_MODEL || DEFAULT_MODEL,
    runs: DEFAULT_RUNS,
    mode: process.env.LOCAL_TRANSLATION_BENCHMARK_MODE || DEFAULT_MODE,
    concurrencies: [1],
    includeUsage: false,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    output: null,
    help: false,
    listSamples: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") {
      config.help = true;
      continue;
    }
    if (argument === "--list-samples") {
      config.listSamples = true;
      continue;
    }
    if (argument === "--include-usage") {
      config.includeUsage = true;
      continue;
    }
    let parsed = optionValue(argv, index, "--endpoint");
    if (parsed) {
      config.endpoint = parsed.value;
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--model");
    if (parsed) {
      config.model = parsed.value;
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--runs");
    if (parsed) {
      config.runs = parseInteger(parsed.value, "--runs", 1, 100);
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--mode");
    if (parsed) {
      config.mode = parsed.value;
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--concurrency");
    if (parsed) {
      config.concurrencies = parseConcurrencyPlan(parsed.value);
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--timeout-ms");
    if (parsed) {
      config.timeoutMs = parseInteger(parsed.value, "--timeout-ms", 1_000, 600_000);
      index += parsed.consumed;
      continue;
    }
    parsed = optionValue(argv, index, "--output");
    if (parsed) {
      config.output = parsed.value;
      index += parsed.consumed;
      continue;
    }
    throw new Error(`Unknown option: ${argument}`);
  }
  return config;
}

function normalizeEndpoint(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("--endpoint must be a valid URL");
  }
  const hostname = url.hostname.toLowerCase();
  if (url.protocol !== "http:") throw new Error("--endpoint must use http");
  if (url.username || url.password) throw new Error("--endpoint must not contain credentials");
  if (!["127.0.0.1", "localhost", "[::1]", "::1"].includes(hostname)) {
    throw new Error("--endpoint must use a loopback hostname");
  }
  if (url.search || url.hash) throw new Error("--endpoint must not contain a query or fragment");
  const path = url.pathname.replace(/\/+$/, "");
  if (path && path !== "/v1") throw new Error("--endpoint path must be /v1 (or empty)");
  url.pathname = "/v1";
  return url.toString().replace(/\/$/, "");
}

function validateModel(value) {
  const model = value.trim();
  if (!model || model.length > 100 || /[\u0000-\u001f\u007f]/.test(model)) {
    throw new Error("--model must contain 1-100 printable characters");
  }
  return model;
}

function promptFor(sample) {
  return [
    `Course: ${sample.course}`,
    "Relevant glossary:",
    ...sample.glossary,
    "Previous sentence for context: (none)",
    `Translate this sentence: ${sample.source}`,
  ].join("\n");
}

function responseContent(value) {
  const content = value?.choices?.[0]?.message?.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => (typeof part === "string" ? part : part?.text))
      .filter((part) => typeof part === "string")
      .join("");
  }
  if (typeof value?.choices?.[0]?.text === "string") return value.choices[0].text;
  if (typeof value?.output_text === "string") return value.output_text;
  return "";
}

function deltaContent(value) {
  const content = value?.choices?.[0]?.delta?.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => (typeof part === "string" ? part : part?.text))
      .filter((part) => typeof part === "string")
      .join("");
  }
  return "";
}

function completionTokenCount(usage) {
  const candidates = [usage?.completion_tokens, usage?.output_tokens, usage?.eval_count];
  return candidates.find((value) => Number.isFinite(value) && value >= 0) ?? null;
}

function heuristicTokenCount(text) {
  const cjk = (text.match(/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/gu) ?? []).length;
  const latinWords = (text.match(/[\p{Letter}\p{Number}]+/gu) ?? [])
    .filter((word) => !/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u.test(word))
    .length;
  const symbols = (text.match(/[^\p{Letter}\p{Number}\s]/gu) ?? []).length;
  return Math.max(1, Math.round(cjk + latinWords * 1.3 + symbols * 0.25));
}

function outputSpeed(text, usage, latencyMs, ttftMs) {
  const reported = completionTokenCount(usage);
  const outputTokens = reported ?? heuristicTokenCount(text);
  const outputCharacters = Array.from(text).length;
  const durationMs = ttftMs === null ? latencyMs : latencyMs - ttftMs;
  const rateTokens = ttftMs === null ? outputTokens : Math.max(0, outputTokens - 1);
  return {
    outputTokens,
    outputCharacters,
    tokenCountSource: reported === null ? "text heuristic" : "response usage",
    timingBasis: ttftMs === null ? "end-to-end wall time" : "first text delta to completion",
    estimatedTokensPerSecond: durationMs > 0 && rateTokens > 0
      ? Number((rateTokens / (durationMs / 1_000)).toFixed(2))
      : null,
    endToEndCharactersPerSecond: latencyMs > 0
      ? Number((outputCharacters / (latencyMs / 1_000)).toFixed(2))
      : null,
  };
}

function compactErrorBody(body) {
  try {
    const value = JSON.parse(body);
    const message = value?.error?.message ?? value?.message ?? body;
    return String(message).slice(0, 500);
  } catch {
    return body.trim().slice(0, 500);
  }
}

async function parseStreamingResponse(response, started) {
  if (!response.body) throw new Error("The endpoint returned an empty streaming body");
  const decoder = new TextDecoder();
  const reader = response.body.getReader();
  let buffer = "";
  let rawBody = "";
  let text = "";
  let usage = null;
  let ttftMs = null;

  const parseLine = (rawLine) => {
    const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
    if (!line.startsWith("data:")) return;
    const data = line.slice(5).trimStart();
    if (!data || data.trim() === "[DONE]") return;
    let value;
    try {
      value = JSON.parse(data);
    } catch {
      throw new Error("The endpoint returned malformed SSE JSON");
    }
    const errorMessage = value?.error?.message ?? (value?.type === "error" ? value?.message : null);
    if (errorMessage) throw new Error(`Streaming error: ${String(errorMessage).slice(0, 500)}`);
    const delta = deltaContent(value);
    if (delta) {
      if (ttftMs === null) ttftMs = performance.now() - started;
      text += delta;
    } else if (!text) {
      const completeText = responseContent(value);
      if (completeText) {
        ttftMs = performance.now() - started;
        text = completeText;
      }
    }
    if (value?.usage) usage = value.usage;
  };

  while (true) {
    const { done, value } = await reader.read();
    const decoded = decoder.decode(value, { stream: !done });
    rawBody += decoded;
    if (rawBody.length > 2_000_000) throw new Error("The streaming response exceeded 2 MB");
    buffer += decoded;
    let lineEnd;
    while ((lineEnd = buffer.indexOf("\n")) !== -1) {
      parseLine(buffer.slice(0, lineEnd));
      buffer = buffer.slice(lineEnd + 1);
    }
    if (done) break;
  }
  if (buffer) parseLine(buffer);
  if (!text && rawBody.trim()) {
    try {
      const value = JSON.parse(rawBody);
      const fallbackText = responseContent(value).trim();
      if (fallbackText) {
        return {
          text: fallbackText,
          usage: value.usage ?? usage,
          ttftMs: null,
          observedResponseMode: "json fallback",
        };
      }
    } catch {
      // Keep the more useful no-translation error emitted by the caller.
    }
  }
  return { text: text.trim(), usage, ttftMs, observedResponseMode: "SSE" };
}

async function parseNonStreamingResponse(response) {
  const body = await response.text();
  let value;
  try {
    value = JSON.parse(body);
  } catch {
    throw new Error("The endpoint returned non-JSON content");
  }
  return {
    text: responseContent(value).trim(),
    usage: value.usage ?? null,
    ttftMs: null,
    observedResponseMode: "JSON",
  };
}

async function requestTranslation(config, sample) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  const started = performance.now();
  try {
    const response = await fetch(`${config.endpoint}/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model: config.model,
        messages: [
          {
            role: "system",
            content: "Translate the lecturer's English into concise, natural Simplified Chinese. Preserve technical terms, numbers, equations, negation, uncertainty, and emphasis. Follow the supplied glossary exactly; when an entry says 保留英文, copy that source token unchanged and do not translate or expand it. Return only the Chinese translation, with no labels, explanation, or visible reasoning.",
          },
          { role: "user", content: promptFor(sample) },
        ],
        stream: config.mode === "stream",
        ...(config.mode === "stream" && config.includeUsage
          ? { stream_options: { include_usage: true } }
          : {}),
        temperature: 0,
        max_tokens: 180,
        reasoning_effort: "none",
      }),
      redirect: "error",
      signal: controller.signal,
    });
    if (!response.ok) {
      const body = await response.text();
      throw new Error(`HTTP ${response.status}: ${compactErrorBody(body)}`);
    }
    const parsed = config.mode === "stream"
      ? await parseStreamingResponse(response, started)
      : await parseNonStreamingResponse(response);
    const latencyMs = performance.now() - started;
    const { text, usage, ttftMs, observedResponseMode } = parsed;
    if (!text) throw new Error("The endpoint returned no translation text");
    return {
      ok: true,
      latencyMs: Number(latencyMs.toFixed(1)),
      ttftMs: ttftMs === null ? null : Number(ttftMs.toFixed(1)),
      text: text.slice(0, 2_000),
      usage,
      observedResponseMode,
      outputSpeed: outputSpeed(text, usage, latencyMs, ttftMs),
    };
  } catch (error) {
    const latencyMs = performance.now() - started;
    const message = error?.name === "AbortError"
      ? `Request timed out after ${config.timeoutMs} ms`
      : String(error?.message ?? error);
    return { ok: false, latencyMs: Number(latencyMs.toFixed(1)), error: message.slice(0, 500) };
  } finally {
    clearTimeout(timer);
  }
}

function strippedTranslation(text) {
  return text
    .replace(/<think>[\s\S]*?<\/think>/gi, "")
    .replace(/^\s*(?:translation|译文|翻译)\s*[:：]\s*/i, "")
    .trim();
}

function normalized(value) {
  return value
    .normalize("NFKC")
    .replace(/[−–—]/g, "-")
    .replace(/[’]/g, "'")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

function formulaNormalized(value) {
  return normalized(value)
    .replace(/\s+/g, "")
    .replace(/lambda/g, "λ")
    .replace(/<=/g, "≤");
}

function containsOne(text, alternatives, normalizer = normalized) {
  const haystack = normalizer(text);
  return alternatives.some((candidate) => haystack.includes(normalizer(candidate)));
}

function containsOneNumber(text, alternatives) {
  const haystack = normalized(text);
  return alternatives.some((candidate) => {
    const needle = normalized(candidate);
    let index = haystack.indexOf(needle);
    while (index !== -1) {
      const before = haystack[index - 1] ?? "";
      const after = haystack[index + needle.length] ?? "";
      if (!/[\d.]/.test(before) && !/[\d.]/.test(after)) return true;
      index = haystack.indexOf(needle, index + 1);
    }
    return false;
  });
}

function scoreTranslation(sample, rawText) {
  const text = strippedTranslation(rawText);
  const termChecks = sample.terms.map((alternatives) => ({
    expectedOneOf: alternatives,
    passed: containsOne(text, alternatives),
  }));
  const numberChecks = sample.numbers.map((alternatives) => ({
    expectedOneOf: alternatives,
    passed: containsOneNumber(text, alternatives),
  }));
  const formulaChecks = sample.formulas.map((alternatives) => ({
    expectedOneOf: alternatives,
    passed: containsOne(text, alternatives, formulaNormalized),
  }));
  const negationPassed = !sample.requiresNegation
    || /(?:不|没|未|并非|并无|不能|不会|不得|不可|无法|切勿|禁止)/.test(text)
    || /\b(?:not|no|never|cannot|can't|doesn't|does\s+not|mustn't|without)\b/i.test(text);
  const outputDisciplinePassed = /\p{Script=Han}/u.test(text)
    && !/<\/?think>|^(?:translation|译文|翻译)\s*[:：]/i.test(rawText.trim());
  return {
    evaluatedText: text,
    termChecks,
    numberChecks,
    formulaChecks,
    negation: { required: sample.requiresNegation, passed: negationPassed },
    outputDisciplinePassed,
  };
}

function nearestRank(values, percentile) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const index = Math.max(0, Math.ceil(percentile * sorted.length) - 1);
  return Number(sorted[index].toFixed(1));
}

function metricSummary(attempts, selector) {
  const values = attempts
    .filter((attempt) => attempt.ok)
    .map(selector)
    .filter((value) => Number.isFinite(value));
  return {
    observations: values.length,
    minimum: values.length ? Number(Math.min(...values).toFixed(1)) : null,
    p50: nearestRank(values, 0.5),
    p95: nearestRank(values, 0.95),
    maximum: values.length ? Number(Math.max(...values).toFixed(1)) : null,
  };
}

function benchmarkSummary(attempts) {
  const successful = attempts.filter((attempt) => attempt.ok).length;
  return {
    requests: attempts.length,
    successfulRequests: successful,
    failedRequests: attempts.length - successful,
    successRate: ratio(successful, attempts.length),
    endToEndLatencyMs: metricSummary(attempts, (attempt) => attempt.latencyMs),
    timeToFirstTextMs: metricSummary(attempts, (attempt) => attempt.ttftMs),
    estimatedOutputTokensPerSecond: metricSummary(
      attempts,
      (attempt) => attempt.outputSpeed?.estimatedTokensPerSecond,
    ),
    endToEndOutputCharactersPerSecond: metricSummary(
      attempts,
      (attempt) => attempt.outputSpeed?.endToEndCharactersPerSecond,
    ),
    preservation: aggregatePreservation(attempts),
  };
}

function ratio(passed, total) {
  return total === 0 ? null : Number((passed / total).toFixed(4));
}

function aggregatePreservation(attempts) {
  const successful = attempts.filter((attempt) => attempt.ok && attempt.score);
  const counters = {
    terms: { passed: 0, total: 0 },
    numbers: { passed: 0, total: 0 },
    formulas: { passed: 0, total: 0 },
    negation: { passed: 0, total: 0 },
    outputDiscipline: { passed: 0, total: 0 },
  };
  for (const attempt of successful) {
    for (const check of attempt.score.termChecks) {
      counters.terms.total += 1;
      counters.terms.passed += Number(check.passed);
    }
    for (const check of attempt.score.numberChecks) {
      counters.numbers.total += 1;
      counters.numbers.passed += Number(check.passed);
    }
    for (const check of attempt.score.formulaChecks) {
      counters.formulas.total += 1;
      counters.formulas.passed += Number(check.passed);
    }
    if (attempt.score.negation.required) {
      counters.negation.total += 1;
      counters.negation.passed += Number(attempt.score.negation.passed);
    }
    counters.outputDiscipline.total += 1;
    counters.outputDiscipline.passed += Number(attempt.score.outputDisciplinePassed);
  }
  return Object.fromEntries(Object.entries(counters).map(([name, counts]) => [name, {
    ...counts,
    rate: ratio(counts.passed, counts.total),
  }]));
}

async function saveOrPrint(result, output) {
  const serialized = `${JSON.stringify(result, null, 2)}\n`;
  process.stdout.write(serialized);
  if (output) await writeFile(output, serialized, "utf8");
}

async function mapWithConcurrency(items, concurrency, task) {
  const results = new Array(items.length);
  let nextIndex = 0;
  const workers = Array.from(
    { length: Math.min(concurrency, items.length) },
    async () => {
      while (nextIndex < items.length) {
        const index = nextIndex;
        nextIndex += 1;
        results[index] = await task(items[index], index);
      }
    },
  );
  await Promise.all(workers);
  return results;
}

async function main() {
  let config;
  try {
    config = parseArguments(process.argv.slice(2));
    if (config.help) {
      process.stdout.write(`${usage()}\n`);
      return;
    }
    if (config.listSamples) {
      process.stdout.write(`${SAMPLES.map((sample) => `${sample.id}\t${sample.source}`).join("\n")}\n`);
      return;
    }
    config.endpoint = normalizeEndpoint(config.endpoint);
    config.model = validateModel(config.model);
    if (config.mode !== "stream" && config.mode !== "non-stream") {
      throw new Error("--mode must be stream or non-stream");
    }
  } catch (error) {
    process.stderr.write(`${error.message}\n\n${usage()}\n`);
    process.exitCode = 1;
    return;
  }

  const startedAt = new Date();
  const attempts = [];
  const coldSample = SAMPLES[0];
  const coldResponse = await requestTranslation(config, coldSample);
  attempts.push({
    phase: "cold",
    pass: 0,
    concurrency: 1,
    sampleId: coldSample.id,
    source: coldSample.source,
    ...coldResponse,
    ...(coldResponse.ok ? { score: scoreTranslation(coldSample, coldResponse.text) } : {}),
  });

  if (coldResponse.ok) {
    for (const concurrency of config.concurrencies) {
      for (let pass = 1; pass <= config.runs; pass += 1) {
        const passAttempts = await mapWithConcurrency(SAMPLES, concurrency, async (sample) => {
          const response = await requestTranslation(config, sample);
          return {
            phase: "warm",
            pass,
            concurrency,
            sampleId: sample.id,
            source: sample.source,
            ...response,
            ...(response.ok ? { score: scoreTranslation(sample, response.text) } : {}),
          };
        });
        attempts.push(...passAttempts);
      }
    }
  }

  const coldAttempts = attempts.filter((attempt) => attempt.phase === "cold");
  const warmAttempts = attempts.filter((attempt) => attempt.phase === "warm");
  const successful = attempts.filter((attempt) => attempt.ok).length;
  const warmByConcurrency = Object.fromEntries(config.concurrencies.map((concurrency) => [
    String(concurrency),
    benchmarkSummary(warmAttempts.filter((attempt) => attempt.concurrency === concurrency)),
  ]));
  const result = {
    schemaVersion: 2,
    generatedAt: new Date().toISOString(),
    elapsedSeconds: Number(((Date.now() - startedAt.getTime()) / 1_000).toFixed(3)),
    environment: {
      node: process.version,
      platform: process.platform,
      architecture: process.arch,
    },
    config: {
      endpoint: config.endpoint,
      model: config.model,
      warmPasses: config.runs,
      mode: config.mode,
      warmConcurrencies: config.concurrencies,
      includeUsage: config.includeUsage,
      timeoutMs: config.timeoutMs,
      requestMode: `${config.mode} chat completions`,
    },
    methodology: {
      cold: "The first request in this process. It is a true cold-load measurement only if the runtime had unloaded the model before the script started.",
      warm: "Each requested concurrency level runs the full sample set after the first request succeeds. Concurrency 2 uses two client workers and includes server queueing in each request's latency.",
      ttft: "Time from fetch start to the first non-empty choices[0].delta.content. It is unavailable in non-stream mode.",
      outputSpeed: "Approximate output tokens divided by wall time after the first text delta (or full wall time in non-stream mode). Token count uses response usage when available and an explicitly labelled text heuristic otherwise. End-to-end Unicode characters per second is also reported as a tokenizer-independent comparison.",
      percentile: "Nearest-rank percentile over successful end-to-end request times.",
      preservation: "Deterministic surface checks over accepted Chinese or retained-English aliases. This is a regression signal, not a semantic-quality score.",
    },
    workload: {
      builtInSamples: SAMPLES.length,
      plannedRequests: 1 + SAMPLES.length * config.runs * config.concurrencies.length,
      attemptedRequests: attempts.length,
    },
    reliability: {
      successfulRequests: successful,
      failedRequests: attempts.length - successful,
      successRate: ratio(successful, attempts.length),
    },
    cold: benchmarkSummary(coldAttempts),
    warmByConcurrency,
    overall: {
      endToEndLatencyMs: metricSummary(attempts, (attempt) => attempt.latencyMs),
      timeToFirstTextMs: metricSummary(attempts, (attempt) => attempt.ttftMs),
      estimatedOutputTokensPerSecond: metricSummary(
        attempts,
        (attempt) => attempt.outputSpeed?.estimatedTokensPerSecond,
      ),
      endToEndOutputCharactersPerSecond: metricSummary(
        attempts,
        (attempt) => attempt.outputSpeed?.endToEndCharactersPerSecond,
      ),
    },
    preservation: aggregatePreservation(attempts),
    attempts,
  };

  try {
    await saveOrPrint(result, config.output);
  } catch (error) {
    process.stderr.write(`Could not write --output file: ${error.message}\n`);
    process.exitCode = 1;
    return;
  }
  if (successful !== attempts.length || warmAttempts.length === 0) process.exitCode = 2;
}

await main();
