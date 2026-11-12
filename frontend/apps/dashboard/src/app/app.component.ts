import { DOCUMENT } from '@angular/common';
import { ChangeDetectionStrategy, Component, effect, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Store } from '@ngrx/store';
import { TuiRoot } from '@taiga-ui/core';
import { selectThemeMode } from './core/state/app-ui.feature';
import { selectResolvedTheme } from './core/state/system-theme';
import { StepUpDialogComponent } from './shared/components/step-up-dialog/step-up-dialog.component';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, StepUpDialogComponent, TuiRoot],
  templateUrl: './app.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent {
  private readonly document = inject(DOCUMENT);
  private readonly store = inject(Store);
  private readonly themeMode = this.store.selectSignal(selectThemeMode);
  protected readonly theme = selectResolvedTheme(this.themeMode);

  constructor() {
    effect(() => this.document.documentElement.setAttribute('data-theme', this.theme()));
  }
}
