import { defineSettings, z } from "@ahab/plugin/shared";

export const settings = defineSettings({
	greeting: z.string().default("hello"),
	loud: z.boolean().default(false),
});
