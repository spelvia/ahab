import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@/shared/styles/globals.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Providers } from "@/app/providers";
import { startAppearance } from "@/shared/appearance";

startAppearance();

const root = document.getElementById("root");
if (!root) throw new Error("The page has no #root element.");

createRoot(root).render(
	<StrictMode>
		<Providers />
	</StrictMode>,
);
