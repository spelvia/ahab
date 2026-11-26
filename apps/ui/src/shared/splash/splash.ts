/**
 * The start-up splash (`#splash` in index.html): plain HTML and CSS, so it
 * paints before the bundle loads. Its logo appears only after 120 ms, so a
 * quick start shows nothing but the page's own background; once the app has
 * drawn its first real screen, the splash fades out (at once with reduced
 * motion, or when the logo never showed).
 */

const FADE_MS = 180;

function reducedMotion() {
	return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** Whether the splash's logo is showing yet. */
function logoShown(splash: HTMLElement) {
	const logo = splash.querySelector("svg");
	if (!logo) return false;
	return Number(window.getComputedStyle(logo).opacity || "0") > 0;
}

/** Hides the splash; safe to call more than once. */
export function hideSplash() {
	const splash = document.getElementById("splash");
	if (!splash || splash.hidden || splash.dataset.leaving) return;
	splash.dataset.leaving = "true";
	if (reducedMotion() || !logoShown(splash)) {
		splash.hidden = true;
		delete splash.dataset.leaving;
		return;
	}
	splash.classList.add("splash-done");
	window.setTimeout(() => {
		if (splash.dataset.leaving) {
			splash.hidden = true;
			delete splash.dataset.leaving;
		}
	}, FADE_MS + 40);
}

/**
 * Shows the splash again with a quiet line under the logo, while something
 * takes long ("Starting Ahab on this computer…"); the desktop app's screens
 * use it.
 */
export function showSplash(status = "") {
	const splash = document.getElementById("splash");
	if (!splash) return;
	delete splash.dataset.leaving;
	splash.classList.remove("splash-done");
	splash.hidden = false;
	const line = document.getElementById("splash-status");
	if (line) line.textContent = status;
}
