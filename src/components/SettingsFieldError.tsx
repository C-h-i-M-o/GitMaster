import type { SettingsError } from "../types/settings";

/** 字段验证失败紧邻控件呈现，跨分类错误仍保留底部定位入口。 */
export function SettingsFieldError({
  error,
  field,
}: {
  error: SettingsError | null;
  field: string | undefined;
}) {
  return (
    <>
      {error?.fieldErrors
        .filter((item) => item.field === field)
        .map((item) => (
          <span className="settings-field-error" role="alert" key={item.field}>
            {item.message}
          </span>
        ))}
    </>
  );
}
