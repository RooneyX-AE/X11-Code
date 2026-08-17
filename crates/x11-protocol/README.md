# x11-protocol

`x11-protocol` defines runtime events, session identifiers, tool-call metadata, streaming events, and interactive approval transport.

## EventBus

`EventBus` is a bounded broadcast stream. Subscribers can observe the same ordered stream independently; a slow subscriber may receive a lag error when the bounded buffer is exceeded.

## ApprovalBroker

Approval requests are keyed by `call_id`. A call ID may have only one pending approval. Duplicate requests are rejected, and pending state is cleaned up when the request channel closes or an approval is resolved.
