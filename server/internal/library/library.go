// Package library stores Lentera profiles on the server and synchronises their lyrics, verses,
// service order, and screen background between devices.
//
// Every change to a profile gets the next number of that profile's revision counter. A device
// remembers the highest revision it has seen (its cursor) and asks for everything after it.
// Conflicts are settled per item by the newest updated_at (last write wins); the losing device
// receives the winning version so all devices end up with the same data. See docs/SINKRON.md.
package library

import (
	"context"
	"errors"
	"fmt"
	"slices"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	// NameMax is the longest profile name, in characters (same as the app).
	NameMax = 40
	// TitleMax is the longest item title, in characters.
	TitleMax = 1000
	// ContentMaxBytes is the largest item content, in UTF-8 bytes.
	ContentMaxBytes = 1 << 20
	// PushMax is the largest number of items one sync request may send.
	PushMax = 1000
	// ServiceMax is the longest service order.
	ServiceMax = 1000
	// BackgroundMax is the highest background index.
	BackgroundMax = 15
	// OwnedProfilesMax is how many profiles one account may own.
	OwnedProfilesMax = 100
	// FutureSkew is how far ahead of the server clock a device timestamp may be before the server
	// replaces it with its own time, so one device with a wrong clock cannot win every conflict.
	FutureSkew = 10 * time.Minute

	// DefaultPageItems and DefaultPageBytes bound one sync response.
	DefaultPageItems = 1000
	DefaultPageBytes = 4 << 20
)

// Roles a member can have in a profile.
const (
	RoleOwner  = "owner"
	RoleEditor = "editor"
	RoleViewer = "viewer"
)

// Kind classifies an Error so the HTTP layer can choose a status code.
type Kind int

const (
	// KindInvalid is a problem with the submitted data (HTTP 422).
	KindInvalid Kind = iota + 1
	// KindNotFound means the profile does not exist or the account is not a member (HTTP 404).
	KindNotFound
	// KindForbidden means the member's role does not allow the change (HTTP 403).
	KindForbidden
	// KindConflict means the request cannot be applied as sent (HTTP 409).
	KindConflict
	// KindGone means the profile was deleted (HTTP 410).
	KindGone
)

// Error is a failure the app can show or act on. Message is in Indonesian, for people.
type Error struct {
	Kind    Kind
	Code    string
	Message string
	Field   string
}

func (e *Error) Error() string { return e.Message }

// Profile is one profile as seen by a member.
type Profile struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	Role          string `json:"role"`
	CreatedAt     int64  `json:"created_at"`
	NameUpdatedAt int64  `json:"name_updated_at"`
	Revision      int64  `json:"revision"`
}

// ProfileInput creates or renames a profile. Times are milliseconds since 1970 (UTC).
type ProfileInput struct {
	Name      string `json:"name"`
	CreatedAt int64  `json:"created_at"`
	UpdatedAt int64  `json:"updated_at"`
}

// Item is one song or verse. A deleted item keeps its id and times but no title or content.
type Item struct {
	ID        string `json:"id"`
	Kind      string `json:"kind"`
	Title     string `json:"title"`
	Content   string `json:"content"`
	CreatedAt int64  `json:"created_at"`
	UpdatedAt int64  `json:"updated_at"`
	DeletedAt *int64 `json:"deleted_at"`
}

// Service is the order of items in the worship service. An item may appear more than once.
type Service struct {
	Items     []string `json:"items"`
	UpdatedAt int64    `json:"updated_at"`
}

// Settings are the profile settings that follow the profile across devices.
type Settings struct {
	Background int   `json:"background"`
	UpdatedAt  int64 `json:"updated_at"`
}

// SyncRequest sends local changes and asks for everything after Cursor.
type SyncRequest struct {
	Cursor   int64     `json:"cursor"`
	Items    []Item    `json:"items"`
	Service  *Service  `json:"service"`
	Settings *Settings `json:"settings"`
}

// SyncResponse carries changes from other devices, and the server's version of anything the
// device sent that lost a conflict or had its time corrected. When More is true the device must
// sync again with the new Cursor; Service and Settings may refer to items on later pages.
type SyncResponse struct {
	Cursor     int64     `json:"cursor"`
	More       bool      `json:"more"`
	Items      []Item    `json:"items"`
	Service    *Service  `json:"service"`
	Settings   *Settings `json:"settings"`
	Accepted   int       `json:"accepted"`
	ServerTime int64     `json:"server_time"`
}

// Store keeps profiles and runs synchronisation. It is safe for concurrent use.
type Store struct {
	pool *pgxpool.Pool
	// Now is the server clock; tests replace it.
	Now func() time.Time
	// PageItems and PageBytes bound one sync response.
	PageItems int
	PageBytes int
}

// NewStore creates the profile store.
func NewStore(pool *pgxpool.Pool) *Store {
	return &Store{pool: pool, Now: time.Now, PageItems: DefaultPageItems, PageBytes: DefaultPageBytes}
}

// Profiles lists the profiles the account is a member of, oldest first.
func (s *Store) Profiles(ctx context.Context, accountID int64) ([]Profile, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT p.id::text, p.name, m.role, p.created_at, p.name_updated_at, p.revision
		FROM profile_members m JOIN profiles p ON p.id = m.profile_id
		WHERE m.account_id = $1 AND p.deleted_at IS NULL
		ORDER BY p.created_at, p.id`, accountID)
	if err != nil {
		return nil, fmt.Errorf("membaca daftar profil: %w", err)
	}
	profiles, err := pgx.CollectRows(rows, func(row pgx.CollectableRow) (Profile, error) {
		var p Profile
		err := row.Scan(&p.ID, &p.Name, &p.Role, &p.CreatedAt, &p.NameUpdatedAt, &p.Revision)
		return p, err
	})
	if err != nil {
		return nil, fmt.Errorf("membaca daftar profil: %w", err)
	}

	return profiles, nil
}

// PutProfile creates the profile with the id the app chose, or renames it when input.UpdatedAt is
// newer than the last rename. It returns the profile as stored and whether it was created.
func (s *Store) PutProfile(ctx context.Context, accountID int64, rawID string, input ProfileInput) (Profile, bool, error) {
	id, invalid := normalizeID(rawID, "id")
	if invalid != nil {
		return Profile{}, false, invalid
	}
	name, invalid := NormalizeName(input.Name)
	if invalid != nil {
		return Profile{}, false, invalid
	}
	if input.CreatedAt <= 0 || input.UpdatedAt <= 0 {
		return Profile{}, false, invalidField("created_at", "Waktu profil tidak valid.")
	}

	var (
		profile Profile
		created bool
		failure *Error
	)
	err := pgx.BeginFunc(ctx, s.pool, func(tx pgx.Tx) error {
		var (
			deleted bool
			role    *string
		)
		err := tx.QueryRow(ctx, `
			SELECT p.name, p.created_at, p.name_updated_at, p.revision, p.deleted_at IS NOT NULL, m.role
			FROM profiles p LEFT JOIN profile_members m ON m.profile_id = p.id AND m.account_id = $2
			WHERE p.id = $1::uuid FOR UPDATE OF p`, id, accountID).
			Scan(&profile.Name, &profile.CreatedAt, &profile.NameUpdatedAt, &profile.Revision, &deleted, &role)
		profile.ID = id
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			return s.createProfile(ctx, tx, accountID, id, name, input, &profile, &created, &failure)
		case err != nil:
			return err
		case deleted:
			failure = gone()
			return nil
		case role == nil:
			failure = &Error{Kind: KindConflict, Code: "profile_taken", Message: "Id profil ini sudah dipakai akun lain."}
			return nil
		case *role == RoleViewer:
			failure = readOnly()
			return nil
		}
		profile.Role = *role

		if input.UpdatedAt > profile.NameUpdatedAt {
			_, err := tx.Exec(ctx, `UPDATE profiles SET name = $2, name_updated_at = $3 WHERE id = $1::uuid`, id, name, input.UpdatedAt)
			if err != nil {
				return err
			}
			profile.Name, profile.NameUpdatedAt = name, input.UpdatedAt
		}
		return nil
	})
	if err != nil {
		return Profile{}, false, fmt.Errorf("menyimpan profil: %w", err)
	}
	if failure != nil {
		return Profile{}, false, failure
	}

	return profile, created, nil
}

func (s *Store) createProfile(ctx context.Context, tx pgx.Tx, accountID int64, id, name string, input ProfileInput, profile *Profile, created *bool, failure **Error) error {
	var owned int
	err := tx.QueryRow(ctx, `
		SELECT count(*) FROM profile_members m JOIN profiles p ON p.id = m.profile_id
		WHERE m.account_id = $1 AND m.role = 'owner' AND p.deleted_at IS NULL`, accountID).Scan(&owned)
	if err != nil {
		return err
	}
	if owned >= OwnedProfilesMax {
		*failure = &Error{Kind: KindInvalid, Code: "profile_limit", Message: fmt.Sprintf("Satu akun paling banyak memiliki %d profil.", OwnedProfilesMax)}
		return nil
	}

	tag, err := tx.Exec(ctx, `
		INSERT INTO profiles (id, name, name_updated_at, created_at, created_by)
		VALUES ($1::uuid, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING`,
		id, name, input.UpdatedAt, input.CreatedAt, accountID)
	if err != nil {
		return err
	}
	if tag.RowsAffected() == 0 {
		// Another request created the same id a moment ago.
		*failure = &Error{Kind: KindConflict, Code: "profile_busy", Message: "Profil sedang dibuat dari perangkat lain. Coba lagi."}
		return nil
	}
	if _, err := tx.Exec(ctx, `INSERT INTO profile_members (profile_id, account_id, role) VALUES ($1::uuid, $2, 'owner')`, id, accountID); err != nil {
		return err
	}

	*profile = Profile{ID: id, Name: name, Role: RoleOwner, CreatedAt: input.CreatedAt, NameUpdatedAt: input.UpdatedAt}
	*created = true
	return nil
}

// DeleteProfile marks the profile deleted for every member. Only the owner may delete it; deleting
// it again does nothing.
func (s *Store) DeleteProfile(ctx context.Context, accountID int64, rawID string) error {
	id, invalid := normalizeID(rawID, "id")
	if invalid != nil {
		return invalid
	}

	var failure *Error
	err := pgx.BeginFunc(ctx, s.pool, func(tx pgx.Tx) error {
		var (
			deleted bool
			role    string
		)
		err := tx.QueryRow(ctx, `
			SELECT p.deleted_at IS NOT NULL, m.role
			FROM profiles p JOIN profile_members m ON m.profile_id = p.id AND m.account_id = $2
			WHERE p.id = $1::uuid FOR UPDATE OF p`, id, accountID).Scan(&deleted, &role)
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			failure = notFound()
			return nil
		case err != nil:
			return err
		case deleted:
			return nil
		case role != RoleOwner:
			failure = &Error{Kind: KindForbidden, Code: "owner_only", Message: "Hanya pemilik profil yang bisa menghapusnya."}
			return nil
		}
		_, err = tx.Exec(ctx, `UPDATE profiles SET deleted_at = now() WHERE id = $1::uuid`, id)
		return err
	})
	if err != nil {
		return fmt.Errorf("menghapus profil: %w", err)
	}
	if failure != nil {
		return failure
	}

	return nil
}

// profileState is the locked profile row during a sync.
type profileState struct {
	revision         int64
	deleted          bool
	role             string
	service          Service
	serviceRevision  int64
	settings         Settings
	settingsRevision int64
}

// Sync applies the device's changes and returns what changed elsewhere since request.Cursor.
func (s *Store) Sync(ctx context.Context, accountID int64, rawID string, request SyncRequest) (SyncResponse, error) {
	id, invalid := normalizeID(rawID, "id")
	if invalid != nil {
		return SyncResponse{}, invalid
	}
	nowMs := s.Now().UnixMilli()
	push, invalid := prepare(request, nowMs+FutureSkew.Milliseconds(), nowMs)
	if invalid != nil {
		return SyncResponse{}, invalid
	}

	var (
		response SyncResponse
		failure  *Error
	)
	err := pgx.BeginFunc(ctx, s.pool, func(tx pgx.Tx) error {
		lock := "FOR SHARE OF p"
		if push.any() {
			lock = "FOR UPDATE OF p"
		}
		var state profileState
		err := tx.QueryRow(ctx, `
			SELECT p.revision, p.deleted_at IS NOT NULL, m.role,
			       p.service::text[], p.service_updated_at, p.service_revision,
			       p.background, p.settings_updated_at, p.settings_revision
			FROM profiles p JOIN profile_members m ON m.profile_id = p.id AND m.account_id = $2
			WHERE p.id = $1::uuid `+lock, id, accountID).
			Scan(&state.revision, &state.deleted, &state.role,
				&state.service.Items, &state.service.UpdatedAt, &state.serviceRevision,
				&state.settings.Background, &state.settings.UpdatedAt, &state.settingsRevision)
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			failure = notFound()
			return nil
		case err != nil:
			return err
		case state.deleted:
			failure = gone()
			return nil
		case push.any() && state.role == RoleViewer:
			failure = readOnly()
			return nil
		case request.Cursor > state.revision:
			failure = &Error{Kind: KindConflict, Code: "cursor_ahead", Field: "cursor", Message: "Data profil di server lebih lama daripada di perangkat ini. Sinkronkan ulang dari awal."}
			return nil
		}

		response, failure, err = s.apply(ctx, tx, id, request.Cursor, state, push)
		return err
	})
	if err != nil {
		return SyncResponse{}, fmt.Errorf("sinkron profil: %w", err)
	}
	if failure != nil {
		return SyncResponse{}, failure
	}
	response.ServerTime = nowMs

	return response, nil
}

// pushed is a validated sync request. adjusted marks values whose time the server replaced.
type pushed struct {
	items            []Item
	adjusted         map[string]bool
	service          *Service
	serviceAdjusted  bool
	settings         *Settings
	settingsAdjusted bool
}

func (p pushed) any() bool { return len(p.items) > 0 || p.service != nil || p.settings != nil }

func (s *Store) apply(ctx context.Context, tx pgx.Tx, id string, cursor int64, state profileState, push pushed) (SyncResponse, *Error, error) {
	response := SyncResponse{Items: []Item{}}
	extra := []Item{}
	next := state.revision

	// Items: the newer updated_at wins; equal times keep what the server has.
	var accepted []Item
	var revisions []int64
	if len(push.items) > 0 {
		stored, err := s.storedItems(ctx, tx, id, push.items)
		if err != nil {
			return response, nil, err
		}
		for _, item := range push.items {
			current, exists := stored[item.ID]
			if exists && item.UpdatedAt <= current.UpdatedAt {
				if !sameItem(item, current) {
					extra = append(extra, current)
				}
				continue
			}
			// The first device's creation time stays; tell the device if it had another one.
			corrected := push.adjusted[item.ID]
			if exists && item.CreatedAt != current.CreatedAt {
				item.CreatedAt = current.CreatedAt
				corrected = true
			}
			next++
			accepted = append(accepted, item)
			revisions = append(revisions, next)
			if corrected {
				extra = append(extra, item)
			}
		}
		if err := upsertItems(ctx, tx, id, accepted, revisions); err != nil {
			return response, nil, err
		}
	}
	response.Accepted = len(accepted)

	if push.service != nil {
		if push.service.UpdatedAt > state.service.UpdatedAt {
			if failure, err := checkServiceItems(ctx, tx, id, push.service.Items); failure != nil || err != nil {
				return response, failure, err
			}
			next++
			_, err := tx.Exec(ctx, `
				UPDATE profiles SET service = $2::text[]::uuid[], service_updated_at = $3, service_revision = $4
				WHERE id = $1::uuid`, id, push.service.Items, push.service.UpdatedAt, next)
			if err != nil {
				return response, nil, err
			}
			response.Accepted++
			if push.serviceAdjusted {
				response.Service = push.service
			}
		} else if !slices.Equal(push.service.Items, state.service.Items) || push.service.UpdatedAt != state.service.UpdatedAt {
			response.Service = &state.service
		}
	} else if state.serviceRevision > cursor {
		response.Service = &state.service
	}

	if push.settings != nil {
		if push.settings.UpdatedAt > state.settings.UpdatedAt {
			next++
			_, err := tx.Exec(ctx, `
				UPDATE profiles SET background = $2, settings_updated_at = $3, settings_revision = $4
				WHERE id = $1::uuid`, id, push.settings.Background, push.settings.UpdatedAt, next)
			if err != nil {
				return response, nil, err
			}
			response.Accepted++
			if push.settingsAdjusted {
				response.Settings = push.settings
			}
		} else if *push.settings != state.settings {
			response.Settings = &state.settings
		}
	} else if state.settingsRevision > cursor {
		response.Settings = &state.settings
	}

	if next > state.revision {
		if _, err := tx.Exec(ctx, `UPDATE profiles SET revision = $2 WHERE id = $1::uuid`, id, next); err != nil {
			return response, nil, err
		}
	}

	// Changes from other devices: everything after the cursor up to the revision the profile had
	// before this request. Revisions after that are this request's own changes.
	page, upto, more, err := s.page(ctx, tx, id, cursor, state.revision)
	if err != nil {
		return response, nil, err
	}
	response.Items = page
	response.More = more
	response.Cursor = next
	if more {
		response.Cursor = upto
	}

	inPage := make(map[string]bool, len(page))
	for _, item := range page {
		inPage[item.ID] = true
	}
	for _, item := range extra {
		if !inPage[item.ID] {
			response.Items = append(response.Items, item)
		}
	}

	return response, nil, nil
}

// page returns items with revision in (cursor, limit], bounded by PageItems and PageBytes. upto is
// the last revision included; more reports whether items after it remain.
func (s *Store) page(ctx context.Context, tx pgx.Tx, id string, cursor, limit int64) ([]Item, int64, bool, error) {
	if cursor >= limit {
		return []Item{}, cursor, false, nil
	}
	rows, err := tx.Query(ctx, `
		SELECT revision, octet_length(title) + octet_length(content)
		FROM items WHERE profile_id = $1::uuid AND revision > $2 AND revision <= $3
		ORDER BY revision LIMIT $4`, id, cursor, limit, s.PageItems+1)
	if err != nil {
		return nil, 0, false, err
	}
	var (
		upto  = cursor
		count int
		bytes int
		more  bool
	)
	for rows.Next() {
		var revision int64
		var size int
		if err := rows.Scan(&revision, &size); err != nil {
			rows.Close()
			return nil, 0, false, err
		}
		size += 200 // id, kind, times, and JSON punctuation
		if count == s.PageItems || (count > 0 && bytes+size > s.PageBytes) {
			more = true
			break
		}
		count++
		bytes += size
		upto = revision
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return nil, 0, false, err
	}
	if count == 0 {
		return []Item{}, cursor, false, nil
	}

	rows, err = tx.Query(ctx, `
		SELECT id::text, kind, title, content, created_at, updated_at, deleted_at
		FROM items WHERE profile_id = $1::uuid AND revision > $2 AND revision <= $3
		ORDER BY revision`, id, cursor, upto)
	if err != nil {
		return nil, 0, false, err
	}
	items, err := pgx.CollectRows(rows, scanItem)
	if err != nil {
		return nil, 0, false, err
	}

	return items, upto, more, nil
}

func (s *Store) storedItems(ctx context.Context, tx pgx.Tx, id string, items []Item) (map[string]Item, error) {
	ids := make([]string, len(items))
	for i, item := range items {
		ids[i] = item.ID
	}
	rows, err := tx.Query(ctx, `
		SELECT id::text, kind, title, content, created_at, updated_at, deleted_at
		FROM items WHERE profile_id = $1::uuid AND id = ANY($2::text[]::uuid[])`, id, ids)
	if err != nil {
		return nil, err
	}
	list, err := pgx.CollectRows(rows, scanItem)
	if err != nil {
		return nil, err
	}
	stored := make(map[string]Item, len(list))
	for _, item := range list {
		stored[item.ID] = item
	}

	return stored, nil
}

func upsertItems(ctx context.Context, tx pgx.Tx, id string, items []Item, revisions []int64) error {
	if len(items) == 0 {
		return nil
	}
	n := len(items)
	ids, kinds, titles, contents := make([]string, n), make([]string, n), make([]string, n), make([]string, n)
	created, updated, deleted := make([]int64, n), make([]int64, n), make([]*int64, n)
	for i, item := range items {
		ids[i], kinds[i], titles[i], contents[i] = item.ID, item.Kind, item.Title, item.Content
		created[i], updated[i], deleted[i] = item.CreatedAt, item.UpdatedAt, item.DeletedAt
	}
	_, err := tx.Exec(ctx, `
		INSERT INTO items (profile_id, id, kind, title, content, created_at, updated_at, deleted_at, revision)
		SELECT $1::uuid, u.id, u.kind, u.title, u.content, u.created_at, u.updated_at, u.deleted_at, u.revision
		FROM unnest($2::text[]::uuid[], $3::text[], $4::text[], $5::text[], $6::bigint[], $7::bigint[], $8::bigint[], $9::bigint[])
		     AS u(id, kind, title, content, created_at, updated_at, deleted_at, revision)
		ON CONFLICT (profile_id, id) DO UPDATE
		SET kind = excluded.kind, title = excluded.title, content = excluded.content,
		    updated_at = excluded.updated_at, deleted_at = excluded.deleted_at, revision = excluded.revision`,
		id, ids, kinds, titles, contents, created, updated, deleted, revisions)

	return err
}

// checkServiceItems makes sure the service order only names items the profile has.
func checkServiceItems(ctx context.Context, tx pgx.Tx, id string, order []string) (*Error, error) {
	unique := slices.Clone(order)
	slices.Sort(unique)
	unique = slices.Compact(unique)
	if len(unique) == 0 {
		return nil, nil
	}
	var found int
	err := tx.QueryRow(ctx, `
		SELECT count(*) FROM items WHERE profile_id = $1::uuid AND id = ANY($2::text[]::uuid[])`,
		id, unique).Scan(&found)
	if err != nil {
		return nil, err
	}
	if found != len(unique) {
		return &Error{Kind: KindInvalid, Code: "service_unknown_item", Field: "service.items",
			Message: "Susunan ibadah memuat materi yang belum ada di server. Kirim materinya lebih dulu."}, nil
	}

	return nil, nil
}

func scanItem(row pgx.CollectableRow) (Item, error) {
	var item Item
	err := row.Scan(&item.ID, &item.Kind, &item.Title, &item.Content, &item.CreatedAt, &item.UpdatedAt, &item.DeletedAt)
	return item, err
}

func sameItem(a, b Item) bool {
	sameDeleted := (a.DeletedAt == nil) == (b.DeletedAt == nil) && (a.DeletedAt == nil || *a.DeletedAt == *b.DeletedAt)
	return a.ID == b.ID && a.Kind == b.Kind && a.Title == b.Title && a.Content == b.Content &&
		a.UpdatedAt == b.UpdatedAt && sameDeleted
}

// prepare validates a sync request, blanks deleted items, and replaces device times that are too
// far in the future with the server time.
func prepare(request SyncRequest, latest, nowMs int64) (pushed, *Error) {
	push := pushed{adjusted: map[string]bool{}}
	if request.Cursor < 0 {
		return push, invalidField("cursor", "Cursor tidak valid.")
	}
	if len(request.Items) > PushMax {
		return push, invalidField("items", fmt.Sprintf("Paling banyak %d materi per permintaan.", PushMax))
	}

	seen := make(map[string]bool, len(request.Items))
	for index, raw := range request.Items {
		field := fmt.Sprintf("items[%d]", index)
		item, invalid := prepareItem(raw, field)
		if invalid != nil {
			return push, invalid
		}
		if seen[item.ID] {
			return push, invalidField(field+".id", "Materi yang sama dikirim dua kali.")
		}
		seen[item.ID] = true
		if item.UpdatedAt > latest {
			item.UpdatedAt = nowMs
			if item.DeletedAt != nil {
				item.DeletedAt = &nowMs
			}
			push.adjusted[item.ID] = true
		}
		push.items = append(push.items, item)
	}

	if request.Service != nil {
		service := Service{Items: make([]string, 0, len(request.Service.Items)), UpdatedAt: request.Service.UpdatedAt}
		if len(request.Service.Items) > ServiceMax {
			return push, invalidField("service.items", fmt.Sprintf("Susunan ibadah paling banyak %d materi.", ServiceMax))
		}
		for index, raw := range request.Service.Items {
			itemID, invalid := normalizeID(raw, fmt.Sprintf("service.items[%d]", index))
			if invalid != nil {
				return push, invalid
			}
			service.Items = append(service.Items, itemID)
		}
		if service.UpdatedAt <= 0 {
			return push, invalidField("service.updated_at", "Waktu susunan ibadah tidak valid.")
		}
		if service.UpdatedAt > latest {
			service.UpdatedAt = nowMs
			push.serviceAdjusted = true
		}
		push.service = &service
	}

	if request.Settings != nil {
		settings := *request.Settings
		if settings.Background < 0 || settings.Background > BackgroundMax {
			return push, invalidField("settings.background", "Pilihan latar tidak valid.")
		}
		if settings.UpdatedAt <= 0 {
			return push, invalidField("settings.updated_at", "Waktu pengaturan tidak valid.")
		}
		if settings.UpdatedAt > latest {
			settings.UpdatedAt = nowMs
			push.settingsAdjusted = true
		}
		push.settings = &settings
	}

	return push, nil
}

func prepareItem(item Item, field string) (Item, *Error) {
	id, invalid := normalizeID(item.ID, field+".id")
	if invalid != nil {
		return Item{}, invalid
	}
	item.ID = id
	if item.Kind != "song" && item.Kind != "verse" {
		return Item{}, invalidField(field+".kind", "Jenis materi harus song atau verse.")
	}
	if item.CreatedAt <= 0 || item.UpdatedAt <= 0 || (item.DeletedAt != nil && *item.DeletedAt <= 0) {
		return Item{}, invalidField(field+".updated_at", "Waktu materi tidak valid.")
	}
	if item.DeletedAt != nil {
		item.Title, item.Content = "", ""
		return item, nil
	}
	if !utf8.ValidString(item.Title) || strings.ContainsRune(item.Title, 0) || utf8.RuneCountInString(item.Title) > TitleMax {
		return Item{}, invalidField(field+".title", fmt.Sprintf("Judul materi paling panjang %d karakter.", TitleMax))
	}
	if !utf8.ValidString(item.Content) || strings.ContainsRune(item.Content, 0) || len(item.Content) > ContentMaxBytes {
		return Item{}, invalidField(field+".content", "Isi materi paling besar 1 MB.")
	}

	return item, nil
}

// NormalizeName collapses whitespace like the app does and checks the length.
func NormalizeName(raw string) (string, *Error) {
	name := strings.Join(strings.Fields(raw), " ")
	switch {
	case name == "":
		return "", invalidField("name", "Nama profil belum diisi.")
	case utf8.RuneCountInString(name) > NameMax:
		return "", invalidField("name", fmt.Sprintf("Nama profil paling panjang %d karakter.", NameMax))
	case strings.ContainsFunc(name, unicode.IsControl):
		return "", invalidField("name", "Nama profil memuat karakter yang tidak didukung.")
	}

	return name, nil
}

// normalizeID accepts a UUID in canonical text form (any letter case) and returns it lower-case.
func normalizeID(raw, field string) (string, *Error) {
	id := strings.ToLower(raw)
	if len(id) != 36 {
		return "", invalidField(field, "Id harus berupa UUID.")
	}
	for i, r := range id {
		switch i {
		case 8, 13, 18, 23:
			if r != '-' {
				return "", invalidField(field, "Id harus berupa UUID.")
			}
		default:
			if !(r >= '0' && r <= '9' || r >= 'a' && r <= 'f') {
				return "", invalidField(field, "Id harus berupa UUID.")
			}
		}
	}

	return id, nil
}

func invalidField(field, message string) *Error {
	return &Error{Kind: KindInvalid, Code: "validation_failed", Field: field, Message: message}
}

func notFound() *Error {
	return &Error{Kind: KindNotFound, Code: "profile_not_found", Message: "Profil tidak ditemukan."}
}

func gone() *Error {
	return &Error{Kind: KindGone, Code: "profile_deleted", Message: "Profil ini sudah dihapus pemiliknya."}
}

func readOnly() *Error {
	return &Error{Kind: KindForbidden, Code: "read_only", Message: "Anda hanya bisa melihat profil ini, tidak mengubahnya."}
}
