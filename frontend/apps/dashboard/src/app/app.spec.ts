import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { AppComponent } from './app.component';

describe('AppComponent', () => {
  it('bootstraps the root shell', async () => {
    await TestBed.configureTestingModule({
      imports: [AppComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideMockStore({
          initialState: { appUi: { themeMode: 'system', sidebarCollapsed: false } },
        }),
        provideZonelessChangeDetection(),
      ],
    }).compileComponents();
    expect(
      TestBed.createComponent(AppComponent).nativeElement.querySelector('tui-root'),
    ).toBeTruthy();
  });
});
