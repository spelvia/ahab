/**
 * The pipe's framing, as language servers do it, with one more header:
 *
 *     Content-Length: <n>\r\n
 *     Ahab-Channel: <uuid>\r\n      (only for a channel's bytes)
 *     \r\n
 *     <n bytes>
 *
 * A message without `Ahab-Channel` is one JSON-RPC 2.0 message in UTF-8; one
 * with it is raw bytes for that channel.
 */

export interface Frame {
	/** The channel the bytes are for; absent for JSON-RPC. */
	channel?: string;
	body: Uint8Array;
}

const SEPARATOR = new TextEncoder().encode("\r\n\r\n");
/** Headers longer than this are not a frame: the stream is broken. */
const MAX_HEADER = 1024;

function indexOf(buffer: Uint8Array, needle: Uint8Array, from = 0): number {
	outer: for (let i = from; i <= buffer.length - needle.length; i++) {
		for (let j = 0; j < needle.length; j++) {
			if (buffer[i + j] !== needle[j]) continue outer;
		}
		return i;
	}
	return -1;
}

/** Encodes a frame. */
export function encode(frame: Frame): Uint8Array {
	let header = `Content-Length: ${frame.body.length}\r\n`;
	if (frame.channel) header += `Ahab-Channel: ${frame.channel}\r\n`;
	header += "\r\n";
	const head = new TextEncoder().encode(header);
	const out = new Uint8Array(head.length + frame.body.length);
	out.set(head, 0);
	out.set(frame.body, head.length);
	return out;
}

/** Reads frames from chunks as they arrive. */
export class Decoder {
	private buffer = new Uint8Array(0);

	/** Adds a chunk; answers the frames it completed. Throws on a broken stream. */
	push(chunk: Uint8Array): Frame[] {
		const joined = new Uint8Array(this.buffer.length + chunk.length);
		joined.set(this.buffer, 0);
		joined.set(chunk, this.buffer.length);
		this.buffer = joined;
		const frames: Frame[] = [];
		for (;;) {
			const end = indexOf(this.buffer, SEPARATOR);
			if (end < 0) {
				if (this.buffer.length > MAX_HEADER) throw new Error("a frame's headers are too long");
				break;
			}
			const headers = new TextDecoder().decode(this.buffer.subarray(0, end));
			let length: number | undefined;
			let channel: string | undefined;
			for (const line of headers.split("\r\n")) {
				const colon = line.indexOf(":");
				if (colon < 0) continue;
				const name = line.slice(0, colon).trim().toLowerCase();
				const value = line.slice(colon + 1).trim();
				if (name === "content-length") length = Number(value);
				else if (name === "ahab-channel") channel = value;
			}
			if (length === undefined || !Number.isInteger(length) || length < 0) {
				throw new Error("a frame has no valid Content-Length");
			}
			const start = end + SEPARATOR.length;
			if (this.buffer.length < start + length) break;
			const body = this.buffer.slice(start, start + length);
			this.buffer = this.buffer.slice(start + length);
			frames.push(channel ? { channel, body } : { body });
		}
		return frames;
	}
}
