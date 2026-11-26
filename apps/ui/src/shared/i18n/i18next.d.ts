import type { resources } from "@/shared/i18n/resources";

// Keys are checked: `t("missing")` is a type error, in every namespace.
declare module "i18next" {
	interface CustomTypeOptions {
		defaultNS: "common";
		resources: (typeof resources)["en"];
	}
}
