import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ApiService } from '../../../core/api/api.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { Skill } from '../../../core/api/tenant-api.models';
import { SkillsManagerComponent } from './skills-manager.component';

describe('SkillsManagerComponent', () => {
  let api: {
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
    delete: ReturnType<typeof vi.fn>;
  };
  let permissions: { has: ReturnType<typeof vi.fn> };

  const mockSkills: Skill[] = [
    { id: 's-1', name: 'billing', agentCount: 3 },
    { id: 's-2', name: 'support', agentCount: 5 },
  ];

  function setup(hasManage: boolean) {
    api = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), delete: vi.fn() };
    permissions = { has: vi.fn(() => hasManage) };

    api.get.mockReturnValue(of({ data: mockSkills }));
    TestBed.configureTestingModule({
      imports: [SkillsManagerComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1', production: false } },
        { provide: ApiService, useValue: api },
        { provide: PermissionsService, useValue: permissions },
      ],
    });
  }

  it('loads skills catalog on init', async () => {
    setup(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('billing');
      expect(fixture.nativeElement.textContent).toContain('support');
    });
  });

  it('shows add/rename/delete controls when user has members.manage', async () => {
    setup(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('.add-form')).toBeTruthy();
      expect(fixture.nativeElement.querySelector('.delete-btn')).toBeTruthy();
    });
  });

  it('shows read-only chips when user lacks members.manage', async () => {
    setup(false);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.textContent).toContain('billing');
    });

    expect(fixture.nativeElement.querySelector('.add-form')).toBeFalsy();
    expect(fixture.nativeElement.querySelector('.delete-btn')).toBeFalsy();
  });

  it('creates a new skill via POST', async () => {
    setup(true);
    api.post.mockReturnValue(of({ data: { id: 's-3', name: 'billing', agentCount: 0 } }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('billing'));

    const input = fixture.nativeElement.querySelector('.add-form input') as HTMLInputElement;
    input.value = 'new-skill';
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));

    await vi.waitFor(() => {
      expect(api.post).toHaveBeenCalledWith('tenant/skills', { name: 'new-skill' });
    });
  });

  it('renames a skill via PATCH', async () => {
    setup(true);
    api.patch.mockReturnValue(of({ data: { id: 's-1', name: 'billing-v2', agentCount: 3 } }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('billing'));

    const nameSpan = fixture.nativeElement.querySelector('.skill-name') as HTMLElement;
    nameSpan.dispatchEvent(new Event('dblclick'));
    fixture.detectChanges();

    const editInput = fixture.nativeElement.querySelector('.skill-item input') as HTMLInputElement;
    editInput.value = 'billing-v2';
    editInput.dispatchEvent(new Event('blur'));

    await vi.waitFor(() => {
      expect(api.patch).toHaveBeenCalledWith('tenant/skills/s-1', { name: 'billing-v2' });
    });
  });

  it('deletes a skill via DELETE', async () => {
    setup(true);
    api.delete.mockReturnValue(of({ data: undefined }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SkillsManagerComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(fixture.nativeElement.textContent).toContain('billing'));

    const deleteBtn = fixture.nativeElement.querySelector('.delete-btn') as HTMLButtonElement;
    deleteBtn.click();

    await vi.waitFor(() => {
      expect(api.delete).toHaveBeenCalledWith('tenant/skills/s-1');
    });
  });
});
