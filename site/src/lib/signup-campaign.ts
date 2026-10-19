// One campaign contract shared by the announcement and invitation entry point.
export const SIGNUP_CAMPAIGN = {
  id: 'early-users-20261025-500',
  cutoff: '2026-10-26T07:00:00Z', // Oct 25, 23:59 America/Los_Angeles.
  credits: 500,
} as const;

const copy = {
  zh: { ends: '活动截止：', title: '首批用户专属好礼', offer: '注册即赠 500 Credits', invite: '邀请活动', close: '关闭活动公告' },
  en: { ends: 'Offer ends in', title: 'A welcome gift for our first users', offer: 'Sign up for 500 free Credits', invite: 'Invite & earn', close: 'Dismiss announcement' },
  ja: { ends: '終了まで', title: '初期ユーザー限定ギフト', offer: '登録で500 Creditsプレゼント', invite: '招待キャンペーン', close: 'お知らせを閉じる' },
  ko: { ends: '이벤트 종료까지', title: '초기 사용자를 위한 특별 선물', offer: '가입하면 500 Credits 증정', invite: '초대 이벤트', close: '공지 닫기' },
  es: { ends: 'La oferta termina en', title: 'Un regalo para nuestros primeros usuarios', offer: 'Regístrate y recibe 500 Credits', invite: 'Invita y gana', close: 'Cerrar anuncio' },
  fr: { ends: 'L’offre se termine dans', title: 'Un cadeau pour nos premiers utilisateurs', offer: 'Inscrivez-vous : 500 Credits offerts', invite: 'Invitez et gagnez', close: 'Fermer l’annonce' },
  de: { ends: 'Angebot endet in', title: 'Ein Geschenk für unsere ersten Nutzer', offer: 'Registrieren und 500 Credits erhalten', invite: 'Freunde einladen', close: 'Ankündigung schließen' },
  pt: { ends: 'A oferta termina em', title: 'Um presente para nossos primeiros usuários', offer: 'Cadastre-se e ganhe 500 Credits', invite: 'Convide e ganhe', close: 'Fechar aviso' },
};

export function campaignCopy(language: string) {
  return copy[language.split('-')[0] as keyof typeof copy] || copy.en;
}

export function campaignCountdown(now = Date.now()): string | null {
  const remaining = Math.ceil((Date.parse(SIGNUP_CAMPAIGN.cutoff) - now) / 1000);
  if (remaining <= 0) return null;
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${pad(Math.floor(remaining / 86400))}d ${pad(Math.floor(remaining / 3600) % 24)}h ${pad(Math.floor(remaining / 60) % 60)}m ${pad(remaining % 60)}s`;
}

export function mountCampaignBanner() {
  const banner = document.querySelector<HTMLElement>('[data-campaign-banner]');
  if (!banner || banner.dataset.mounted) return;
  banner.dataset.mounted = 'true';
  const key = `socai-dismissed:${SIGNUP_CAMPAIGN.id}`;
  let dismissed = false;
  try { dismissed = localStorage.getItem(key) === '1'; } catch { /* Private browsing still supports dismissing this visit. */ }
  const resize = () => {
    document.documentElement.style.setProperty('--campaign-banner-height', `${banner.hidden ? 0 : banner.getBoundingClientRect().height}px`);
  };
  let timer: ReturnType<typeof setInterval> | undefined;
  const update = () => {
    const countdown = campaignCountdown();
    banner.hidden = dismissed || !countdown;
    document.body.classList.toggle('has-campaign-banner', !banner.hidden);
    if (banner.hidden) {
      resize();
      if (timer) clearInterval(timer);
      return;
    }
    const language = document.documentElement.dataset.language || document.documentElement.lang || 'en';
    const text = campaignCopy(language);
    banner.querySelector<HTMLElement>('[data-campaign-title]')!.textContent = text.title;
    banner.querySelector<HTMLElement>('[data-campaign-offer]')!.textContent = text.offer;
    banner.querySelector<HTMLElement>('[data-campaign-countdown]')!.textContent = `${text.ends} ${countdown}`;
    banner.querySelector<HTMLElement>('[data-campaign-countdown]')!.title = '2026-10-25 23:59 · America/Los_Angeles';
    banner.querySelector('[data-campaign-close]')!.setAttribute('aria-label', text.close);
    const link = banner.querySelector<HTMLAnchorElement>('[data-campaign-link]')!;
    link.href = `https://${language.startsWith('zh') ? 'agentcn' : 'agent'}.socai.io/session?lang=${language.split('-')[0]}`;
    resize();
  };
  banner.querySelector<HTMLButtonElement>('[data-campaign-close]')!.onclick = () => {
    dismissed = true;
    try { localStorage.setItem(key, '1'); } catch { /* Keep in-memory dismissal. */ }
    update();
  };
  new ResizeObserver(resize).observe(banner);
  new MutationObserver(update).observe(document.documentElement, { attributes: true, attributeFilter: ['lang', 'data-language'] });
  update();
  if (!banner.hidden) timer = setInterval(update, 1000);
}
