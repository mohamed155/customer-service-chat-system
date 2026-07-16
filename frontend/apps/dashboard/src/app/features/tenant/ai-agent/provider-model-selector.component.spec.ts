import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import {
  ProviderModelSelectorComponent,
  ProviderOption,
} from './provider-model-selector.component';

const MOCK_PROVIDERS: ProviderOption[] = [
  { id: 'openai', name: 'OpenAI', credentialAvailable: true, models: ['gpt-4o', 'gpt-4o-mini'] },
  {
    id: 'anthropic',
    name: 'Anthropic',
    credentialAvailable: true,
    models: ['claude-4', 'claude-3-haiku'],
  },
  { id: 'google', name: 'Google AI', credentialAvailable: false, models: ['gemini-2'] },
];

describe('ProviderModelSelectorComponent', () => {
  function createComponent(providers?: ProviderOption[], stale?: boolean) {
    TestBed.configureTestingModule({
      imports: [ProviderModelSelectorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ProviderModelSelectorComponent);
    if (providers) fixture.componentRef.setInput('providers', providers);
    if (stale !== undefined) fixture.componentRef.setInput('stale', stale);
    fixture.detectChanges();
    return fixture;
  }

  it('only shows providers with credentials available', () => {
    const fixture = createComponent(MOCK_PROVIDERS);
    const options = fixture.nativeElement.querySelectorAll(
      'select:first-of-type option',
    ) as NodeListOf<HTMLOptionElement>;
    const texts = Array.from(options).map((o) => o.textContent);
    expect(texts).toContain('OpenAI');
    expect(texts).toContain('Anthropic');
    expect(texts).not.toContain('Google AI');
  });

  it('includes a "Follow platform default" option', () => {
    const fixture = createComponent(MOCK_PROVIDERS);
    const options = fixture.nativeElement.querySelectorAll('select:first-of-type option');
    expect(options[0].textContent).toContain('Follow platform default');
  });

  it('shows stale warning when stale input is true', () => {
    const fixture = createComponent(MOCK_PROVIDERS, true);
    expect(fixture.nativeElement.textContent).toContain('stale');
  });

  it('does not show stale warning when stale input is false', () => {
    const fixture = createComponent(MOCK_PROVIDERS, false);
    expect(fixture.nativeElement.textContent).not.toContain('stale');
  });

  it('shows model select when a provider is selected', () => {
    TestBed.configureTestingModule({
      imports: [ProviderModelSelectorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ProviderModelSelectorComponent);
    fixture.componentRef.setInput('providers', MOCK_PROVIDERS);
    fixture.componentRef.setInput('value', { providerId: 'openai', model: null });
    fixture.detectChanges();

    const labels = fixture.nativeElement.querySelectorAll('.label') as NodeListOf<HTMLElement>;
    const modelLabel = Array.from(labels).find((l) => l.textContent === 'Model');
    expect(modelLabel).toBeTruthy();
  });
});
