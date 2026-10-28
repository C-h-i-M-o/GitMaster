import type { OperationFeedbackModel } from "../ui/operationFeedback";
import { useOperationFeedback } from "../hooks/useOperationFeedback";
import "./operationFeedback.css";

/** 共用审核通过的分支汇流造型，真实结果决定消失或错误恢复。 */
export function OperationFeedback({
  model,
}: {
  model: OperationFeedbackModel;
}) {
  const clock = useOperationFeedback(model);
  return (
    <section
      className={`operation-feedback ${model.compact ? "compact" : "centered"} ${model.error ? "failed" : "pending"}`}
      aria-label={model.title}
    >
      <div className="feedback-art" aria-hidden="true">
        {model.error ? (
          <svg className="feedback-failure" viewBox="0 0 156 112" fill="none">
            <ellipse
              cx="77"
              cy="99"
              rx="51"
              ry="6"
              fill="#dedacb"
              opacity=".35"
            />
            <rect
              x="33"
              y="17"
              width="91"
              height="65"
              rx="10"
              fill="#eef3ea"
              stroke="#c5d5c4"
              transform="rotate(7 78 50)"
            />
            <path
              d="M26 37a8 8 0 0 1 8-8h25l10 10h52a8 8 0 0 1 8 8v38a8 8 0 0 1-8 8H34a8 8 0 0 1-8-8Z"
              fill="#faf7ed"
              stroke="#c6b28e"
              strokeWidth="1.5"
            />
            <path
              d="M48 72h15m22 0h17"
              stroke="#819d8c"
              strokeWidth="2"
              strokeLinecap="round"
            />
            <path
              d="m68 67 4 5-4 5m11-10-4 5 4 5"
              stroke="#ac8752"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <circle
              cx="46"
              cy="72"
              r="4"
              fill="#f9fbf5"
              stroke="#6d9883"
              strokeWidth="1.5"
            />
            <circle
              cx="104"
              cy="72"
              r="4"
              fill="#f9fbf5"
              stroke="#6d9883"
              strokeWidth="1.5"
            />
            <circle cx="126" cy="37" r="14" fill="#f3e7ce" stroke="#ceb27c" />
            <path
              d="M126 30v8m0 5v.1"
              stroke="#966c31"
              strokeWidth="2"
              strokeLinecap="round"
            />
          </svg>
        ) : (
          <svg className="feedback-flow" viewBox="0 0 224 156">
            <path className="feedback-rail teal" d="M112 135V28" />
            <path
              className="feedback-rail blue"
              d="M44 112V90C44 62 112 70 112 28"
            />
            <path
              className="feedback-rail gold"
              d="M180 112V90C180 62 112 70 112 28"
            />
            <path className="feedback-trace teal" d="M112 135V28" />
            <path
              className="feedback-trace blue"
              d="M44 112V90C44 62 112 70 112 28"
            />
            <path
              className="feedback-trace gold"
              d="M180 112V90C180 62 112 70 112 28"
            />
            <circle r="3" className="feedback-traveler" />
            <circle r="3" className="feedback-traveler blue" />
            <circle r="3" className="feedback-traveler gold" />
            <circle className="feedback-node blue" cx="44" cy="112" r="6" />
            <circle className="feedback-node gold" cx="180" cy="112" r="6" />
            <circle className="feedback-node teal" cx="112" cy="135" r="5" />
            <circle className="feedback-ring" cx="112" cy="28" r="19" />
            <circle
              cx="112"
              cy="28"
              r="13"
              fill="#eef6ef"
              stroke="#6a9d8d"
              strokeWidth="1.4"
            />
            <circle className="feedback-core" cx="112" cy="28" r="7" />
            <circle cx="109" cy="25" r="2" fill="white" opacity=".9" />
          </svg>
        )}
      </div>
      <div className="feedback-copy">
        <div role={model.error ? "alert" : "status"}>
          <h2>{model.title}</h2>
          {model.description && <p>{model.description}</p>}
        </div>
        <div className="feedback-meta">
          <i />
          {model.phase && (
            <>
              <span>{model.phase}</span>
              <span>·</span>
            </>
          )}
          <time aria-live="off">{clock.duration}</time>
        </div>
        {clock.long && <p className="feedback-long">{clock.longTip}</p>}
        {model.action && (
          <button className="primary feedback-action" onClick={model.action}>
            {model.actionLabel}
          </button>
        )}
      </div>
    </section>
  );
}
