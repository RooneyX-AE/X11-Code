# x11-model

Provider-neutral model transport for X11 Code.

## Contract

`ModelProvider` receives a validated `CompletionRequest` and returns a structured `CompletionResponse`.

`OpenAiCompatible` targets an OpenAI-compatible `/chat/completions` endpoint. It validates the base URL, request schema, tool-call JSON arguments, response size, and malformed tool calls before data reaches the agent runtime.

Transient HTTP retries are intentionally narrow: `429`, `502`, `503`, and `504` are retried with bounded exponential backoff. Other HTTP failures are surfaced immediately.

The provider does not interpret arbitrary tool-call text as executable input. Tool-call arguments must be valid JSON objects.

## Safety limits

Model names, response text, and tool-call argument payloads are bounded. The HTTP client also has connection and request timeouts.

The request transport adds `x-x11-request-id` so provider-side logs can correlate retries belonging to one logical completion request.

## Mock provider

`MockProvider` is deterministic and intended for unit/integration tests that must not contact a real model service.
