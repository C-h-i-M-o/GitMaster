import CredentialProvider, {
  type CredentialRef,
  type CredentialInfo,
  type CredentialKey,
  type CredentialRecord,
  type CredentialRecordInfo,
  type CredentialRecordEntry,
} from "@deepseek-ai/dsh-credentials";
import type { Context } from "@deepseek-ai/cordis";

/** 凭据值仅来自受信任的桌面桥接，不读取环境变量或项目文件。 */
export interface CredentialBridge {
  resolve: (ref: string) => Promise<string | undefined>;
}

/** 为上游凭据 seam 提供只读桥接；写入始终由系统凭据存储负责。 */
export class DesktopCredentials extends CredentialProvider {
  private readonly bridge: CredentialBridge;
  /** 保存桥接句柄，不在普通配置中保存密钥。 */
  constructor(ctx: Context, bridge: CredentialBridge) {
    super(ctx);
    this.bridge = bridge;
  }
  /** 每个模型请求独立解析引用，空配置使用明确的无认证占位。 */
  async resolve(ref: CredentialRef) {
    const value =
      ref === "GITMASTER_AI_NO_KEY" ? "no-key" : await this.bridge.resolve(ref);
    return value ? { value, source: "system-keyring" } : undefined;
  }
  /** 仅返回是否已配置，已存明文不出现在描述符中。 */
  async describe(ref: CredentialRef): Promise<CredentialInfo> {
    return { configured: Boolean(await this.resolve(ref)), writable: false };
  }
  /** 不向上游插件开放凭据修改入口。 */
  async set(): Promise<void> {
    throw new Error("AI_CREDENTIAL_WRITE_DENIED");
  }
  /** 删除与更新均由桌面保存事务完成。 */
  async unset(): Promise<void> {
    throw new Error("AI_CREDENTIAL_WRITE_DENIED");
  }
  /** 首版没有 OAuth 账号记录，不查询用户环境中的其他账号。 */
  async readRecord(_key: CredentialKey): Promise<CredentialRecord | undefined> {
    return undefined;
  }
  /** 缺少账号记录时提供明确的不可写描述。 */
  async describeRecord(_key: CredentialKey): Promise<CredentialRecordInfo> {
    return { configured: false, writable: false };
  }
  /** 不枚举系统中的其他凭据。 */
  async listRecords(): Promise<readonly CredentialRecordEntry[]> {
    return [];
  }
  /** 首版固定为 API 密钥认证，拒绝隐式账号写入。 */
  async modifyRecord(): Promise<CredentialRecord | undefined> {
    throw new Error("AI_CREDENTIAL_WRITE_DENIED");
  }
  /** 不允许插件删除账号凭据。 */
  async deleteRecord(): Promise<void> {
    throw new Error("AI_CREDENTIAL_WRITE_DENIED");
  }
}
