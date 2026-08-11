export type RegionInfo = {
  label: string;
  flag: string;
};

type RegionRule = {
  label: string;
  flag: string;
  keywords: string[];
};

// Ordered by specificity; first match wins. Keywords are matched case-insensitively
// against the node name (which usually carries country names, codes, or emoji).
const RULES: RegionRule[] = [
  { label: "中国香港", flag: "🇭🇰", keywords: ["香港", "hong kong", "hongkong", "hk", "🇭🇰"] },
  { label: "中国台湾", flag: "🇨🇳", keywords: ["台湾", "臺灣", "taiwan", "tw", "🇹🇼"] },
  { label: "中国澳门", flag: "🇲🇴", keywords: ["澳门", "macao", "macau", "mo", "🇲🇴"] },
  { label: "日本", flag: "🇯🇵", keywords: ["日本", "japan", "tokyo", "东京", "大阪", "jp", "🇯🇵"] },
  { label: "新加坡", flag: "🇸🇬", keywords: ["新加坡", "singapore", "sg", "狮城", "🇸🇬"] },
  { label: "韩国", flag: "🇰🇷", keywords: ["韩国", "韓國", "korea", "seoul", "首尔", "kr", "🇰🇷"] },
  { label: "美国", flag: "🇺🇸", keywords: ["美国", "united states", "usa", "us", "洛杉矶", "圣何塞", "硅谷", "纽约", "🇺🇸"] },
  { label: "英国", flag: "🇬🇧", keywords: ["英国", "united kingdom", "london", "伦敦", "uk", "gb", "🇬🇧"] },
  { label: "德国", flag: "🇩🇪", keywords: ["德国", "germany", "frankfurt", "法兰克福", "de", "🇩🇪"] },
  { label: "法国", flag: "🇫🇷", keywords: ["法国", "france", "paris", "巴黎", "fr", "🇫🇷"] },
  { label: "荷兰", flag: "🇳🇱", keywords: ["荷兰", "netherlands", "nl", "🇳🇱"] },
  { label: "俄罗斯", flag: "🇷🇺", keywords: ["俄罗斯", "russia", "moscow", "ru", "🇷🇺"] },
  { label: "加拿大", flag: "🇨🇦", keywords: ["加拿大", "canada", "ca", "🇨🇦"] },
  { label: "澳大利亚", flag: "🇦🇺", keywords: ["澳大利亚", "australia", "sydney", "悉尼", "au", "🇦🇺"] },
  { label: "印度", flag: "🇮🇳", keywords: ["印度", "india", "mumbai", "in", "🇮🇳"] },
  { label: "土耳其", flag: "🇹🇷", keywords: ["土耳其", "turkey", "tr", "🇹🇷"] },
  { label: "阿根廷", flag: "🇦🇷", keywords: ["阿根廷", "argentina", "ar", "🇦🇷"] },
  { label: "巴西", flag: "🇧🇷", keywords: ["巴西", "brazil", "br", "🇧🇷"] },
  { label: "马来西亚", flag: "🇲🇾", keywords: ["马来", "malaysia", "my", "🇲🇾"] },
  { label: "泰国", flag: "🇹🇭", keywords: ["泰国", "thailand", "th", "🇹🇭"] },
  { label: "越南", flag: "🇻🇳", keywords: ["越南", "vietnam", "vn", "🇻🇳"] },
  { label: "中国", flag: "🇨🇳", keywords: ["中国", "china", "cn", "回国", "🇨🇳"] },
];

const FALLBACK: RegionInfo = { label: "未知", flag: "🌐" };

export function detectRegion(name: string): RegionInfo {
  const lower = name.toLowerCase();
  for (const rule of RULES) {
    if (rule.keywords.some((keyword) => lower.includes(keyword.toLowerCase()))) {
      return { label: rule.label, flag: rule.flag };
    }
  }
  return FALLBACK;
}

export const REGION_OPTIONS = Array.from(
  new Set(RULES.map((rule) => rule.label)),
);
