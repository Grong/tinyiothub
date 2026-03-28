# DEPRECATED

> **This standalone MCP server is deprecated as of 2026-03-28.**

## Reason

Phase 1 of the Edge Intelligence Agent implementation has moved MCP tools to an embedded server inside the main `api` crate at `api/src/api/mcp/`. This separate crate is no longer maintained.

## Migration

Users should migrate to the embedded MCP endpoint at `/mcp` on the main API server (port 8080 by default).

## Old Architecture (Deprecated)

This crate was a standalone MCP server that:
- Ran as a separate process
- Required separate deployment and monitoring
- Used JSON-RPC over HTTP for tool communication

## New Architecture (Recommended)

The embedded MCP server:
- Runs inside the main `tinyiothub` binary
- No separate process or deployment needed
- Integrated with the application's lifecycle and logging

## Status

**Abandoned** - This crate is kept for reference only and will be removed in a future release.
