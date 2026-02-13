# Runtime API

`minimax-cli` includes a local runtime HTTP/SSE API for durable threads, turns, events, and background tasks.

## Start Server

```bash
minimax serve --http --host 127.0.0.1 --port 7878 --workers 2
```

Notes:
- MCP stdio mode remains available: `minimax serve --mcp`
- Runtime data persists under `~/.minimax/tasks` by default.
- Override task storage with `MINIMAX_TASKS_DIR`.

## Endpoints

### Health and Sessions
- `GET /health`
- `GET /v1/sessions`

### Streaming
- `POST /v1/stream`

### Threads
- `GET /v1/threads`
- `POST /v1/threads`
- `GET /v1/threads/{id}`
- `POST /v1/threads/{id}/resume`
- `POST /v1/threads/{id}/fork`
- `POST /v1/threads/{id}/turns`
- `POST /v1/threads/{id}/turns/{turn_id}/steer`
- `POST /v1/threads/{id}/turns/{turn_id}/interrupt`
- `POST /v1/threads/{id}/compact`
- `GET /v1/threads/{id}/events`

### Tasks
- `GET /v1/tasks`
- `POST /v1/tasks`
- `GET /v1/tasks/{id}`
- `POST /v1/tasks/{id}/cancel`

## Example

Create a thread and start a turn:

```bash
curl -s http://127.0.0.1:7878/v1/threads \
  -H 'content-type: application/json' \
  -d '{}'

curl -s http://127.0.0.1:7878/v1/threads/<thread_id>/turns \
  -H 'content-type: application/json' \
  -d '{"prompt":"Summarize the latest changes"}'
```

Consume replayable SSE events:

```bash
curl -N "http://127.0.0.1:7878/v1/threads/<thread_id>/events?since_seq=0"
```
