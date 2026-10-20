import type { Parameter } from "../../ipc/bindings";

/**
 * Renders one input per declared parameter. Values fall back to the
 * parameter's default; required ones are marked. Shared by the Skills and
 * Scripts run panels.
 */
export function ParamFields({
  parameters,
  values,
  onChange,
  disabled,
}: {
  parameters: Parameter[];
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
  disabled?: boolean;
}) {
  if (parameters.length === 0) {
    return (
      <p className="text-xs text-neutral-500">
        This one takes no parameters.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2">
      {parameters.map((param) => (
        <label key={param.name} className="flex flex-col gap-1">
          <span className="text-xs font-medium text-neutral-500">
            {param.name}
            {param.required && <span className="text-red-500"> *</span>}
            {param.description && (
              <span className="ml-1 font-normal text-neutral-400">
                {param.description}
              </span>
            )}
          </span>
          <textarea
            value={values[param.name] ?? param.default ?? ""}
            disabled={disabled}
            rows={2}
            onChange={(event) => onChange(param.name, event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none disabled:opacity-50 dark:border-neutral-600"
          />
        </label>
      ))}
    </div>
  );
}
