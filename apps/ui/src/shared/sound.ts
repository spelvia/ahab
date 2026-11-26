/**
 * Short tones made in the browser (Web Audio): no sound files to fetch or
 * license. Browsers let a page play sound only after it was clicked or typed
 * in once; until then a tone is skipped.
 */

export type Tone = "chime" | "ping" | "soft";

export const TONES: readonly Tone[] = ["chime", "ping", "soft"];

/** Each tone's notes: pitch (Hz), start and length (s), and its wave. */
const NOTES: Record<Tone, { pitch: number; at: number; length: number; wave: OscillatorType }[]> = {
	chime: [
		{ pitch: 880, at: 0, length: 0.25, wave: "sine" },
		{ pitch: 1318.5, at: 0.12, length: 0.4, wave: "sine" },
	],
	ping: [{ pitch: 1568, at: 0, length: 0.3, wave: "sine" }],
	soft: [
		{ pitch: 523.3, at: 0, length: 0.35, wave: "triangle" },
		{ pitch: 659.3, at: 0.1, length: 0.45, wave: "triangle" },
	],
};

let context: AudioContext | undefined;

function audio(): AudioContext | undefined {
	if (typeof window === "undefined" || typeof window.AudioContext === "undefined") return undefined;
	context ??= new window.AudioContext();
	return context;
}

/** Plays `tone` at `volume` (0 to 1); nothing where the browser does not allow it yet. */
export function playTone(tone: Tone, volume = 0.5) {
	const sound = audio();
	if (!sound) return;
	if (sound.state === "suspended") void sound.resume();
	const start = sound.currentTime + 0.01;
	for (const note of NOTES[tone]) {
		const oscillator = sound.createOscillator();
		const gain = sound.createGain();
		oscillator.type = note.wave;
		oscillator.frequency.value = note.pitch;
		// A quick rise, then a fade: no click at either end.
		const from = start + note.at;
		gain.gain.setValueAtTime(0, from);
		gain.gain.linearRampToValueAtTime(0.25 * volume, from + 0.01);
		gain.gain.exponentialRampToValueAtTime(0.0001, from + note.length);
		oscillator.connect(gain).connect(sound.destination);
		oscillator.start(from);
		oscillator.stop(from + note.length + 0.05);
	}
}
