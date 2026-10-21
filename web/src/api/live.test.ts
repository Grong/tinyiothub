import { describe, expect, it } from 'vitest';
import { parseSseChunk } from './live.js';

describe('parseSseChunk', () => {
  it('parses id/event/multi-line data', () => {
    const { events, rest } = parseSseChunk(
      'id: 42\nevent: state_change\ndata: {"v":1}\ndata: {"x":2}\n\n',
    );
    expect(rest).toBe('');
    expect(events).toHaveLength(1);
    expect(events[0].id).toBe('42');
    expect(events[0].event).toBe('state_change');
    expect(events[0].data).toBe('{"v":1}\n{"x":2}');
  });

  it('keeps an incomplete frame in rest', () => {
    const { events, rest } = parseSseChunk(
      'event: watch\ndata: {"a":1}\n\nevent: job\ndata: {"b"',
    );
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('watch');
    expect(rest).toBe('event: job\ndata: {"b"');
  });

  it('handles CRLF and ignores comments/keepalive', () => {
    const { events } = parseSseChunk(': ping\r\n\r\nevent: snapshot\r\ndata: {}\r\n\r\n');
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('snapshot');
    expect(events[0].data).toBe('{}');
  });

  it('defaults event name to message', () => {
    const { events } = parseSseChunk('data: hello\n\n');
    expect(events[0].event).toBe('message');
  });
});
