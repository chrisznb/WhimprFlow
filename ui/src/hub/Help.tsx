import { t } from "../i18n";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle } from "./ui";
import { Icon, type IconName } from "./icons";

const TIPS: () => { icon: IconName; title: string; body: string }[] = () => [
  { icon: "mic", title: t("hp.t1"), body: t("hp.b1") },
  { icon: "transforms", title: t("hp.t2"), body: t("hp.b2") },
  { icon: "dictionary", title: t("hp.t3"), body: t("hp.b3") },
  { icon: "assistant", title: t("hp.t4"), body: t("hp.b4") },
  { icon: "settings", title: t("hp.t5"), body: t("hp.b5") },
];

export function Help() {
  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle sub={t("hp.sub")}>{t("hp.title")}</PageTitle>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {TIPS().map((tip) => (
          <Card key={tip.title}>
            <div style={{ display: "flex", gap: 14 }}>
              <Icon name={tip.icon} size={22} style={{ color: theme.accentDeep }} />
              <div>
                <div
                  style={{
                    fontFamily: font.ui,
                    fontSize: 15,
                    fontWeight: 600,
                    color: theme.textStrong,
                    marginBottom: 4,
                  }}
                >
                  {tip.title}
                </div>
                <div style={{ fontSize: 13.5, lineHeight: 1.55, color: theme.textMuted }}>{tip.body}</div>
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
