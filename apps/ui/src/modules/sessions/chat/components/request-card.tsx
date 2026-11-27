import { FormRequest } from "@/modules/sessions/chat/components/form-request";
import { PermissionRequest } from "@/modules/sessions/chat/components/permission-request";
import { PlanRequest } from "@/modules/sessions/chat/components/plan-request";
import { QuestionRequest } from "@/modules/sessions/chat/components/question-request";
import type { RequestProps } from "@/modules/sessions/chat/components/request-frame";
import { shownKind } from "@/modules/sessions/chat/model";

/**
 * Something the agent asks, in the conversation where it asked: a
 * permission, a question, a plan to approve or a form, each as its own
 * component. Answered, it stays, showing what was chosen.
 */
export function RequestCard(props: RequestProps) {
	switch (shownKind(props.request)) {
		case "question":
			return <QuestionRequest key={props.request.id} {...props} />;
		case "planApproval":
			return <PlanRequest {...props} />;
		case "form":
			return <FormRequest {...props} />;
		default:
			return <PermissionRequest {...props} />;
	}
}
