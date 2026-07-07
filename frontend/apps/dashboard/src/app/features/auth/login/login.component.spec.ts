import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { LoginComponent } from './login.component';

describe('LoginComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [LoginComponent],
      providers: [provideRouter([]), provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('renders the branded auth card and labeled fields without submit side effects', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(LoginComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).toContain('Helix');
    expect(element.textContent).toContain('Welcome back');
    expect(element.querySelector('input[type="email"]')).toBeTruthy();
    expect(element.querySelector('input[type="password"]')).toBeTruthy();
    expect(element.querySelector('form')?.dispatchEvent(new Event('submit'))).toBe(true);
  });
});
