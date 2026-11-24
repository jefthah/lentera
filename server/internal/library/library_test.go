package library_test

import (
	"context"
	"errors"
	"fmt"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/jefthah/lentera/server/internal/library"
	"github.com/jefthah/lentera/server/internal/testdb"
)

// now is the fixed server clock in these tests.
var now = time.UnixMilli(1_800_000_000_000)

func uid(n int) string { return fmt.Sprintf("0190d2a0-0000-7000-8000-%012d", n) }

func ms(offset int64) int64 { return now.UnixMilli() + offset }

func setup(t *testing.T) (*library.Store, *pgxpool.Pool) {
	t.Helper()
	pool := testdb.New(t)
	store := library.NewStore(pool)
	store.Now = func() time.Time { return now }
	return store, pool
}

func account(t *testing.T, pool *pgxpool.Pool, email string) int64 {
	t.Helper()
	var id int64
	if err := pool.QueryRow(context.Background(), `INSERT INTO accounts (email) VALUES ($1) RETURNING id`, email).Scan(&id); err != nil {
		t.Fatal(err)
	}
	return id
}

func member(t *testing.T, pool *pgxpool.Pool, profile string, accountID int64, role string) {
	t.Helper()
	_, err := pool.Exec(context.Background(), `INSERT INTO profile_members (profile_id, account_id, role) VALUES ($1::uuid, $2, $3)`, profile, accountID, role)
	if err != nil {
		t.Fatal(err)
	}
}

func createProfile(t *testing.T, store *library.Store, accountID int64, id, name string) {
	t.Helper()
	if _, _, err := store.PutProfile(context.Background(), accountID, id, library.ProfileInput{Name: name, CreatedAt: ms(-1000), UpdatedAt: ms(-1000)}); err != nil {
		t.Fatal(err)
	}
}

func song(n int, title string, updated int64) library.Item {
	return library.Item{ID: uid(n), Kind: "song", Title: title, Content: "Bait " + title, CreatedAt: ms(-5000), UpdatedAt: updated}
}

func sync(t *testing.T, store *library.Store, accountID int64, profile string, request library.SyncRequest) library.SyncResponse {
	t.Helper()
	response, err := store.Sync(context.Background(), accountID, profile, request)
	if err != nil {
		t.Fatalf("sync: %v", err)
	}
	return response
}

func failure(t *testing.T, err error, code string) {
	t.Helper()
	var e *library.Error
	if !errors.As(err, &e) || e.Code != code {
		t.Fatalf("err = %v, want %q", err, code)
	}
}

func titles(items []library.Item) []string {
	out := make([]string, len(items))
	for i, item := range items {
		out[i] = item.Title
	}
	return out
}

func TestProfilesAreCreatedRenamedAndListed(t *testing.T) {
	ctx := context.Background()
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	ani := account(t, pool, "ani@gereja.org")

	profile, created, err := store.PutProfile(ctx, budi, strings.ToUpper(uid(1)), library.ProfileInput{Name: "  Ibadah   Umum ", CreatedAt: ms(-100), UpdatedAt: ms(-100)})
	if err != nil || !created || profile.ID != uid(1) || profile.Name != "Ibadah Umum" || profile.Role != library.RoleOwner {
		t.Fatalf("create = %+v, %v, %v", profile, created, err)
	}

	// An older rename loses; a newer one wins.
	profile, created, err = store.PutProfile(ctx, budi, uid(1), library.ProfileInput{Name: "Lama", CreatedAt: ms(-100), UpdatedAt: ms(-200)})
	if err != nil || created || profile.Name != "Ibadah Umum" {
		t.Fatalf("older rename = %+v, %v, %v", profile, created, err)
	}
	profile, _, err = store.PutProfile(ctx, budi, uid(1), library.ProfileInput{Name: "Ibadah Pagi ✓", CreatedAt: ms(-100), UpdatedAt: ms(0)})
	if err != nil || profile.Name != "Ibadah Pagi ✓" || profile.NameUpdatedAt != ms(0) {
		t.Fatalf("newer rename = %+v, %v", profile, err)
	}

	_, _, err = store.PutProfile(ctx, ani, uid(1), library.ProfileInput{Name: "Punya Ani", CreatedAt: 1, UpdatedAt: 1})
	failure(t, err, "profile_taken")
	_, _, err = store.PutProfile(ctx, budi, uid(2), library.ProfileInput{Name: " ", CreatedAt: 1, UpdatedAt: 1})
	failure(t, err, "validation_failed")
	_, _, err = store.PutProfile(ctx, budi, uid(2), library.ProfileInput{Name: strings.Repeat("x", 41), CreatedAt: 1, UpdatedAt: 1})
	failure(t, err, "validation_failed")
	_, _, err = store.PutProfile(ctx, budi, "../profil", library.ProfileInput{Name: "x", CreatedAt: 1, UpdatedAt: 1})
	failure(t, err, "validation_failed")

	if _, _, err := store.PutProfile(ctx, budi, uid(2), library.ProfileInput{Name: "Komisi Pemuda", CreatedAt: ms(50), UpdatedAt: ms(50)}); err != nil {
		t.Fatal(err)
	}
	list, err := store.Profiles(ctx, budi)
	if err != nil || len(list) != 2 || list[0].Name != "Ibadah Pagi ✓" || list[1].Name != "Komisi Pemuda" {
		t.Fatalf("Profiles(budi) = %+v, %v", list, err)
	}
	if list, err := store.Profiles(ctx, ani); err != nil || len(list) != 0 {
		t.Fatalf("Profiles(ani) = %+v, %v", list, err)
	}
}

func TestTwoDevicesSeeEachOthersChanges(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")

	// Device A uploads its library.
	a := sync(t, store, budi, uid(100), library.SyncRequest{
		Items:    []library.Item{song(1, "Satu", ms(-50)), song(2, "Dua ✓", ms(-40))},
		Service:  &library.Service{Items: []string{uid(2), uid(1), uid(2)}, UpdatedAt: ms(-30)},
		Settings: &library.Settings{Background: 2, UpdatedAt: ms(-20)},
	})
	if a.Accepted != 4 || a.Cursor != 4 || len(a.Items) != 0 || a.Service != nil || a.Settings != nil || a.More || a.ServerTime != now.UnixMilli() {
		t.Fatalf("first push = %+v", a)
	}

	// Device B starts empty and receives everything.
	b := sync(t, store, budi, uid(100), library.SyncRequest{})
	if b.Cursor != 4 || !slices.Equal(titles(b.Items), []string{"Satu", "Dua ✓"}) || b.More {
		t.Fatalf("first pull = %+v", b)
	}
	if b.Service == nil || !slices.Equal(b.Service.Items, []string{uid(2), uid(1), uid(2)}) || b.Settings == nil || b.Settings.Background != 2 {
		t.Fatalf("pull service/settings = %+v / %+v", b.Service, b.Settings)
	}
	if b.Items[0].CreatedAt != ms(-5000) || b.Items[0].UpdatedAt != ms(-50) || b.Items[0].DeletedAt != nil {
		t.Fatalf("item times = %+v", b.Items[0])
	}

	// B edits one song and deletes the other.
	deleted := ms(-5)
	b = sync(t, store, budi, uid(100), library.SyncRequest{
		Cursor: b.Cursor,
		Items: []library.Item{
			song(1, "Satu (baru)", ms(-10)),
			{ID: uid(2), Kind: "song", Title: "Dua ✓", Content: "masih ada", CreatedAt: ms(-5000), UpdatedAt: deleted, DeletedAt: &deleted},
		},
		Service: &library.Service{Items: []string{uid(1)}, UpdatedAt: ms(-5)},
	})
	if b.Accepted != 3 || b.Cursor != 7 || len(b.Items) != 0 {
		t.Fatalf("B push = %+v", b)
	}

	// A catches up: the edit, a blank tombstone, and the new service order.
	a = sync(t, store, budi, uid(100), library.SyncRequest{Cursor: a.Cursor})
	if a.Cursor != 7 || len(a.Items) != 2 || a.Settings != nil || a.Service == nil || !slices.Equal(a.Service.Items, []string{uid(1)}) {
		t.Fatalf("A pull = %+v", a)
	}
	if a.Items[0].Title != "Satu (baru)" || a.Items[1].DeletedAt == nil || a.Items[1].Title != "" || a.Items[1].Content != "" {
		t.Fatalf("A pull items = %+v", a.Items)
	}

	// Nothing new: an empty answer with the same cursor.
	a = sync(t, store, budi, uid(100), library.SyncRequest{Cursor: a.Cursor})
	if a.Cursor != 7 || len(a.Items) != 0 || a.Service != nil {
		t.Fatalf("idle sync = %+v", a)
	}
	list, _ := store.Profiles(context.Background(), budi)
	if list[0].Revision != 7 {
		t.Fatalf("profile revision = %d", list[0].Revision)
	}
}

func TestTheNewestChangeWinsAndTheLoserLearnsIt(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")
	sync(t, store, budi, uid(100), library.SyncRequest{
		Items:    []library.Item{song(1, "Asli", ms(-100))},
		Settings: &library.Settings{Background: 1, UpdatedAt: ms(-100)},
	})

	// A edits at -20, then B (offline since cursor 2) sends an older edit from -50.
	sync(t, store, budi, uid(100), library.SyncRequest{Cursor: 2, Items: []library.Item{song(1, "Versi A", ms(-20))}})
	b := sync(t, store, budi, uid(100), library.SyncRequest{
		Cursor:   2,
		Items:    []library.Item{song(1, "Versi B", ms(-50))},
		Settings: &library.Settings{Background: 2, UpdatedAt: ms(-200)},
	})
	if b.Accepted != 0 || b.Cursor != 3 || !slices.Equal(titles(b.Items), []string{"Versi A"}) {
		t.Fatalf("losing push = %+v", b)
	}
	if b.Settings == nil || b.Settings.Background != 1 {
		t.Fatalf("losing settings = %+v", b.Settings)
	}

	// Sending what the server already has changes nothing and returns nothing.
	again := sync(t, store, budi, uid(100), library.SyncRequest{Cursor: 3, Items: []library.Item{song(1, "Versi A", ms(-20))}})
	if again.Accepted != 0 || again.Cursor != 3 || len(again.Items) != 0 {
		t.Fatalf("repeat push = %+v", again)
	}
}

func TestOwnChangesAreNotEchoedButOthersAre(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")
	sync(t, store, budi, uid(100), library.SyncRequest{Items: []library.Item{song(1, "Dari A", ms(-30))}})

	// B has never synced and pushes its own song: it gets A's song, not its own back.
	b := sync(t, store, budi, uid(100), library.SyncRequest{Items: []library.Item{song(2, "Dari B", ms(-20))}})
	if b.Accepted != 1 || b.Cursor != 2 || !slices.Equal(titles(b.Items), []string{"Dari A"}) {
		t.Fatalf("B = %+v", b)
	}
}

func TestDeviceTimesTooFarAheadUseServerTime(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")

	future := ms(int64(24 * time.Hour / time.Millisecond))
	response := sync(t, store, budi, uid(100), library.SyncRequest{
		Items:    []library.Item{song(1, "Jam salah", future), song(2, "Jam benar", ms(5*60_000))},
		Service:  &library.Service{Items: []string{uid(1)}, UpdatedAt: future},
		Settings: &library.Settings{Background: 1, UpdatedAt: future},
	})
	if response.Accepted != 4 || len(response.Items) != 1 || response.Items[0].ID != uid(1) || response.Items[0].UpdatedAt != now.UnixMilli() {
		t.Fatalf("items = %+v", response.Items)
	}
	if response.Service == nil || response.Service.UpdatedAt != now.UnixMilli() || response.Settings == nil || response.Settings.UpdatedAt != now.UnixMilli() {
		t.Fatalf("service/settings = %+v / %+v", response.Service, response.Settings)
	}
}

func TestConcurrentDevicesGetDistinctRevisions(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")

	const devices, perDevice = 8, 25
	errs := make(chan error, devices)
	for d := range devices {
		go func() {
			var err error
			for i := range perDevice {
				n := 1 + d*perDevice + i
				_, err = store.Sync(context.Background(), budi, uid(100), library.SyncRequest{Items: []library.Item{song(n, fmt.Sprint(n), ms(-1))}})
				if err != nil {
					break
				}
			}
			errs <- err
		}()
	}
	for range devices {
		if err := <-errs; err != nil {
			t.Fatal(err)
		}
	}

	store.PageItems = 10_000
	pull := sync(t, store, budi, uid(100), library.SyncRequest{})
	seen := map[string]bool{}
	for _, item := range pull.Items {
		seen[item.ID] = true
	}
	if pull.Cursor != devices*perDevice || len(pull.Items) != devices*perDevice || len(seen) != devices*perDevice {
		t.Fatalf("cursor %d, %d items, %d unique", pull.Cursor, len(pull.Items), len(seen))
	}
	var distinct int
	if err := pool.QueryRow(context.Background(), `SELECT count(DISTINCT revision) FROM items`).Scan(&distinct); err != nil || distinct != devices*perDevice {
		t.Fatalf("distinct revisions = %d, %v", distinct, err)
	}
}

func TestServiceMayOnlyNameKnownItems(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")

	_, err := store.Sync(context.Background(), budi, uid(100), library.SyncRequest{
		Items:   []library.Item{song(1, "Satu", ms(-10))},
		Service: &library.Service{Items: []string{uid(1), uid(9)}, UpdatedAt: ms(-5)},
	})
	failure(t, err, "service_unknown_item")

	// The failed request changed nothing.
	pull := sync(t, store, budi, uid(100), library.SyncRequest{})
	if pull.Cursor != 0 || len(pull.Items) != 0 {
		t.Fatalf("after failed push = %+v", pull)
	}
}

func TestLargeLibrariesArriveInPages(t *testing.T) {
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")
	var items []library.Item
	for n := 1; n <= 5; n++ {
		items = append(items, song(n, fmt.Sprintf("Lagu %d", n), ms(int64(-100+n))))
	}
	sync(t, store, budi, uid(100), library.SyncRequest{Items: items, Service: &library.Service{Items: []string{uid(5)}, UpdatedAt: ms(-1)}})

	store.PageItems = 2
	var got []string
	cursor := int64(0)
	for range 10 {
		page := sync(t, store, budi, uid(100), library.SyncRequest{Cursor: cursor})
		got = append(got, titles(page.Items)...)
		cursor = page.Cursor
		if !page.More {
			break
		}
		if len(page.Items) != 2 || page.Service == nil {
			t.Fatalf("page = %+v", page)
		}
	}
	if cursor != 6 || len(got) != 5 || got[4] != "Lagu 5" {
		t.Fatalf("pages gave %v, cursor %d", got, cursor)
	}

	// Pages are also cut by size.
	store.PageItems = 1000
	store.PageBytes = 1
	page := sync(t, store, budi, uid(100), library.SyncRequest{})
	if len(page.Items) != 1 || !page.More || page.Cursor != 1 {
		t.Fatalf("size-limited page = %+v", page)
	}
}

func TestRolesAndDeletedProfiles(t *testing.T) {
	ctx := context.Background()
	store, pool := setup(t)
	owner := account(t, pool, "pemilik@gereja.org")
	editor := account(t, pool, "editor@gereja.org")
	viewer := account(t, pool, "lihat@gereja.org")
	stranger := account(t, pool, "lain@gereja.org")
	createProfile(t, store, owner, uid(100), "Ibadah Umum")
	member(t, pool, uid(100), editor, library.RoleEditor)
	member(t, pool, uid(100), viewer, library.RoleViewer)

	sync(t, store, editor, uid(100), library.SyncRequest{Items: []library.Item{song(1, "Dari editor", ms(-10))}})
	if pull := sync(t, store, viewer, uid(100), library.SyncRequest{}); len(pull.Items) != 1 {
		t.Fatalf("viewer pull = %+v", pull)
	}
	_, err := store.Sync(ctx, viewer, uid(100), library.SyncRequest{Items: []library.Item{song(1, "Dari penonton", ms(-1))}})
	failure(t, err, "read_only")
	_, _, err = store.PutProfile(ctx, viewer, uid(100), library.ProfileInput{Name: "Ganti", CreatedAt: 1, UpdatedAt: ms(0)})
	failure(t, err, "read_only")
	_, err = store.Sync(ctx, stranger, uid(100), library.SyncRequest{})
	failure(t, err, "profile_not_found")
	_, err = store.Sync(ctx, owner, uid(100), library.SyncRequest{Cursor: 99})
	failure(t, err, "cursor_ahead")

	failure(t, store.DeleteProfile(ctx, editor, uid(100)), "owner_only")
	failure(t, store.DeleteProfile(ctx, stranger, uid(100)), "profile_not_found")
	if err := store.DeleteProfile(ctx, owner, uid(100)); err != nil {
		t.Fatal(err)
	}
	if err := store.DeleteProfile(ctx, owner, uid(100)); err != nil {
		t.Fatalf("second delete: %v", err)
	}
	_, err = store.Sync(ctx, editor, uid(100), library.SyncRequest{})
	failure(t, err, "profile_deleted")
	_, _, err = store.PutProfile(ctx, owner, uid(100), library.ProfileInput{Name: "Lagi", CreatedAt: 1, UpdatedAt: ms(1)})
	failure(t, err, "profile_deleted")
	if list, _ := store.Profiles(ctx, editor); len(list) != 0 {
		t.Fatalf("deleted profile still listed: %+v", list)
	}
}

func TestInvalidPushesAreRejectedWhole(t *testing.T) {
	ctx := context.Background()
	store, pool := setup(t)
	budi := account(t, pool, "budi@gereja.org")
	createProfile(t, store, budi, uid(100), "Ibadah Umum")

	tooMany := make([]library.Item, library.PushMax+1)
	for i := range tooMany {
		tooMany[i] = song(i+1, "x", ms(-1))
	}
	cases := map[string]library.SyncRequest{
		"negative cursor": {Cursor: -1},
		"bad id":          {Items: []library.Item{{ID: "bukan-uuid", Kind: "song", CreatedAt: 1, UpdatedAt: 1}}},
		"bad kind":        {Items: []library.Item{{ID: uid(1), Kind: "video", CreatedAt: 1, UpdatedAt: 1}}},
		"no time":         {Items: []library.Item{{ID: uid(1), Kind: "song", CreatedAt: 1}}},
		"same id twice":   {Items: []library.Item{song(1, "a", ms(-2)), song(1, "b", ms(-1))}},
		"nul in title":    {Items: []library.Item{song(1, "a\x00b", ms(-1))}},
		"long title":      {Items: []library.Item{song(1, strings.Repeat("é", library.TitleMax+1), ms(-1))}},
		"too many":        {Items: tooMany},
		"bad background":  {Settings: &library.Settings{Background: 16, UpdatedAt: 1}},
		"bad service id":  {Service: &library.Service{Items: []string{"x"}, UpdatedAt: 1}},
	}
	for name, request := range cases {
		_, err := store.Sync(ctx, budi, uid(100), request)
		failure(t, err, "validation_failed")
		_ = name
	}
	// NUL disebut sebagai karakter yang tidak sah, bukan sebagai ukuran.
	nulContent := song(1, "a", ms(-1))
	nulContent.Content = "Bait\x00"
	for field, item := range map[string]library.Item{
		"title":   song(1, "a\x00b", ms(-1)),
		"content": nulContent,
	} {
		_, err := store.Sync(ctx, budi, uid(100), library.SyncRequest{Items: []library.Item{item}})
		var e *library.Error
		if !errors.As(err, &e) || !strings.HasSuffix(e.Field, "."+field) || !strings.Contains(e.Message, "karakter yang tidak sah") {
			t.Fatalf("NUL in %s: err = %+v", field, err)
		}
	}
	if pull := sync(t, store, budi, uid(100), library.SyncRequest{}); pull.Cursor != 0 {
		t.Fatalf("invalid pushes changed the profile: %+v", pull)
	}
}
