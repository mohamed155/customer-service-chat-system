import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';
import { FormsModule } from '@angular/forms';
import { ApiService } from '../../../core/api/api.service';
import { Skill } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';

@Component({
  selector: 'app-skills-manager',
  imports: [TuiIcon, FormsModule],
  template: `
    <div class="skills-manager">
      <div class="header">
        <h3>Skill Catalog</h3>
        @if (canManage()) {
          <div class="add-form">
            <input
              #newName
              type="text"
              placeholder="New skill name"
              (keydown.enter)="addSkill(newName.value); newName.value = ''"
            />
            <button
              type="button"
              class="add-btn"
              (click)="addSkill(newName.value); newName.value = ''"
            >
              <tui-icon icon="@tui.plus" />
            </button>
          </div>
        }
      </div>
      <ul class="skill-list">
        @for (skill of skills(); track skill.id) {
          <li class="skill-item">
            @if (editingId() === skill.id) {
              <input
                #editInput
                [value]="skill.name"
                (keydown.enter)="renameSkill(skill.id, editInput.value)"
                (keydown.escape)="editingId.set(null)"
                (blur)="renameSkill(skill.id, editInput.value)"
              />
            } @else {
              <span class="skill-name" (dblclick)="startEdit(skill.id)">{{ skill.name }}</span>
            }
            <span class="skill-count"
              >{{ skill.agentCount }} agent{{ skill.agentCount === 1 ? '' : 's' }}</span
            >
            @if (canManage()) {
              <button
                type="button"
                class="delete-btn"
                [attr.aria-label]="'Delete skill ' + skill.name"
                (click)="deleteSkill(skill.id)"
              >
                <tui-icon icon="@tui.trash" />
              </button>
            }
          </li>
        } @empty {
          <li class="empty">No skills defined yet.</li>
        }
      </ul>
    </div>
  `,
  styles: [
    `
      .skills-manager {
        display: grid;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
      }
      .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--app-space-2);
      }
      h3 {
        margin: 0;
        font-size: var(--app-font-base);
        font-weight: 700;
        color: var(--app-text);
      }
      .add-form {
        display: flex;
        gap: 6px;
      }
      .add-form input {
        height: 32px;
        padding: 0 8px;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-sm);
        background: var(--app-panel);
        color: var(--app-text);
        font-size: var(--app-font-sm);
      }
      .add-btn {
        width: 32px;
        height: 32px;
        display: grid;
        place-items: center;
        border: 1px solid var(--app-accent);
        border-radius: var(--app-radius-sm);
        background: var(--app-accent);
        color: var(--app-accent-ink);
        cursor: pointer;
      }
      .skill-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
      }
      .skill-item {
        display: flex;
        align-items: center;
        gap: var(--app-space-3);
        padding: var(--app-space-2) 0;
        border-bottom: 1px solid var(--app-border);
      }
      .skill-item:last-child {
        border-bottom: none;
      }
      .skill-name {
        flex: 1;
        font-weight: 600;
        color: var(--app-text);
        cursor: pointer;
      }
      .skill-count {
        color: var(--app-text-3);
        font-size: var(--app-font-xs);
      }
      .delete-btn {
        width: 28px;
        height: 28px;
        display: grid;
        place-items: center;
        border: 0;
        border-radius: var(--app-radius-sm);
        background: transparent;
        color: var(--app-text-3);
        cursor: pointer;
      }
      .delete-btn:hover {
        background: var(--app-danger-bg, rgba(220, 38, 38, 0.1));
        color: var(--app-danger, #dc2626);
      }
      .empty {
        color: var(--app-text-3);
        font-style: italic;
        padding: var(--app-space-2) 0;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SkillsManagerComponent {
  private readonly api = inject(ApiService);
  private readonly permissions = inject(PermissionsService);
  readonly skills = signal<Skill[]>([]);
  readonly editingId = signal<string | null>(null);
  readonly canManage = () => this.permissions.has('conversations.manage');

  constructor() {
    this.loadSkills();
  }

  private loadSkills(): void {
    this.api.get<Skill[]>('tenant/skills').subscribe((res) => {
      this.skills.set(res.data);
    });
  }

  addSkill(name: string): void {
    const trimmed = name.trim();
    if (!trimmed) return;
    this.api.post<Skill>('tenant/skills', { name: trimmed }).subscribe({
      next: (res) => {
        this.skills.update((list) => [...list, res.data]);
      },
    });
  }

  renameSkill(id: string, name: string): void {
    this.editingId.set(null);
    const trimmed = name.trim();
    if (!trimmed) return;
    this.api.patch<Skill>(`tenant/skills/${id}`, { name: trimmed }).subscribe({
      next: (res) => {
        this.skills.update((list) => list.map((s) => (s.id === id ? res.data : s)));
      },
    });
  }

  deleteSkill(id: string): void {
    this.api.delete<void>(`tenant/skills/${id}`).subscribe({
      next: () => {
        this.skills.update((list) => list.filter((s) => s.id !== id));
      },
    });
  }

  startEdit(id: string): void {
    this.editingId.set(id);
  }
}
