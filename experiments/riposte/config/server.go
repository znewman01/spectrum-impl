package db

import (
	"bytes"
	"crypto/tls"
	"errors"
	"fmt"
	"log"
	"math"
	"net/rpc"
	"time"

	"bitbucket.org/henrycg/riposte/utils"
	"bitbucket.org/henrycg/zkp/group"
)

// Time to wait between merges (in seconds)
const MERGE_TIME_DELAY time.Duration = 60 * 60 * 24

var (
	incomingReqs = make(chan [NUM_SERVERS]EncryptedInsertQuery, REQ_BUFFER_SIZE)
	auditReqs    = make(chan AuditArgs, REQ_BUFFER_SIZE)
	commitReqs   = make(chan CommitArgs, REQ_BUFFER_SIZE)

	beginMergeMarker       [NUM_SERVERS]EncryptedInsertQuery
	beginMergeMarkerAudit  AuditArgs
	beginMergeMarkerCommit CommitArgs
)

func (t *Server) isLeader() bool {
	return (t.ServerIdx == 0)
}

// Upload from client to leader
// Check validity of first msg
// Send msg to other nodes

// First phase from leader to node PREPARE
// Check validity of msg

// First phase from node to leader VOTE
// Add to pending pool or return NO

// Second phase from leader to node COMMIT

// Second phase from node to leader ACK

/*******************
 * Leader code
 */

func (t *Server) Upload(args *UploadArgs, reply *UploadReply) error {
	if !t.isLeader() {
		return errors.New("Only leader can accept uploads")
	}

	log.Printf("Got upload request")
	//log.Printf("Request:", args)

	if t.State != State_AcceptUpload {
		return errors.New("Not accepting uploads")
	}

	incomingReqs <- args.Query
	reply.Magic = 5
	return nil
}

func readIncomingRequests(preps *[NUM_SERVERS]PrepareArgs,
	c chan [NUM_SERVERS]EncryptedInsertQuery, shouldMerge *bool) int {

	// Read once, then read until we get to
	// the end

	n := 0
	appendOnce := func(queryList [NUM_SERVERS]EncryptedInsertQuery) {
		for i := 0; i < NUM_SERVERS; i++ {
			(*preps)[i].Queries = append((*preps)[i].Queries, queryList[i])
		}
		n++
	}

	isEmpty := func(q [NUM_SERVERS]EncryptedInsertQuery) bool {
		return q[0].Ciphertext == nil
	}

	first := <-c
	if isEmpty(first) {
		return n
	} else {
		appendOnce(first)
	}

	//time.Sleep(1*time.Second)

	*shouldMerge = false
	for {
		select {
		// Each element of incomingReqs has an array
		// of queries -- one for each server
		case queryList := <-c:
			// If we're starting to merge, then the marker down
			// the pipeline
			if isEmpty(queryList) {
				*shouldMerge = true
				return n
			} else {
				appendOnce(queryList)
				if len(preps[0].Queries) >= MAX_QUERY_SIZE {
					return n
				}
			}
			continue

		// Wait until we get MAX_QUERY_SIZE queries
		default:
			//continue
			return n
		}
	}

	return n
}

func (t *Server) submitPrepares() {
	for {
		uuid, err := utils.RandomInt64(math.MaxInt64)
		if err != nil {
			log.Printf("error in random")
			continue
		}

		var preps [NUM_SERVERS]PrepareArgs
		for i := 0; i < NUM_SERVERS; i++ {
			preps[i].Uuid = uuid
			preps[i].Queries = make([]EncryptedInsertQuery, 0)
		}

		var shouldMerge bool
		n := readIncomingRequests(&preps, incomingReqs, &shouldMerge)
		if n == 0 {
			// Merge is starting, so send marker down pipeline
			auditReqs <- beginMergeMarkerAudit
		}

		log.Printf("Send PREPARE %d", uuid)

		// Send out PREPARE request
		c := make(chan error, NUM_SERVERS)
		var replies [NUM_SERVERS]PrepareReply
		for i := 0; i < NUM_SERVERS; i++ {
			go func(prep *PrepareArgs, reply *PrepareReply, j int) {
				err := t.rpcClients[j].Call("Server.Prepare", prep, reply)
				if err != nil {
					c <- err
				} else {
					c <- nil
				}
			}(&preps[i], &replies[i], i)
		}

		// Wait for responses
		var r error
		for i := 0; i < NUM_SERVERS; i++ {
			r = <-c
			if r != nil {
				log.Fatal("Error in prepare: ", r)
			}
		}

		var auditArgs AuditArgs
		auditArgs.Uuid = uuid
		if len(replies[0].QueryToAudit) != len(replies[1].QueryToAudit) {
			panic("Length mismatch")
		}

		auditList := make([][NUM_SERVERS]EncryptedAuditQuery,
			len(replies[0].QueryToAudit))
		auditArgs.QueriesToAudit = auditList

		okay := true
		for i := 0; i < NUM_SERVERS; i++ {
			if !replies[i].Okay {
				log.Printf("Aborting commit")
				okay = false
			}

			for j := range replies[i].QueryToAudit {
				auditArgs.QueriesToAudit[j][i] = replies[i].QueryToAudit[j]
			}
		}

		log.Printf("Done PREPARE %d", uuid)

		// If merge is beginning, send marker down
		// the pipeline
		if shouldMerge {
			auditReqs <- beginMergeMarkerAudit
		}

		if okay {
			log.Printf("Putting audit request in pipeline %d", uuid)
			auditReqs <- auditArgs
		} else {
			log.Printf("Telling servers to abort the commit %d", uuid)
			// Tell servers to abort the commit
			var commitArgs CommitArgs
			commitArgs.Uuid = preps[0].Uuid
			commitArgs.Commit = make([]bool, len(replies[0].QueryToAudit))
			commitReqs <- commitArgs
		}
	}
}

func (t *Server) submitAudits() {
	for {
		req := <-auditReqs
		log.Printf("Send AUDIT %d", req.Uuid)
		if req.Uuid == beginMergeMarkerCommit.Uuid {
			commitReqs <- beginMergeMarkerCommit
			continue
		}

		// Send out AUDIT request
		var a_reply AuditReply
		err := t.rpcClients[AUDIT_SERVER].Call("Auditor.Audit", req, &a_reply)
		if err != nil {
			log.Fatal("Error in audit: ", err)
		}

		var commitArgs CommitArgs
		commitArgs.Uuid = req.Uuid
		commitArgs.Commit = a_reply.Okay
		commitReqs <- commitArgs
		for i := range a_reply.Okay {
			if !a_reply.Okay[i] {
				log.Printf("FAILED audit, uuid: %v[%v]", req.Uuid, i)
			}
		}

		log.Printf("Done AUDIT %d => Okay? %v", req.Uuid, a_reply.Okay)
	}
}

func (t *Server) submitCommits() {
	for {
		com := <-commitReqs
		if com.Uuid == beginMergeMarkerCommit.Uuid {
			t.sendMergeRequest()
		}
		log.Printf("Send COMMIT %d", com.Uuid)

		// Send out COMMIT request
		c := make(chan error, NUM_SERVERS)
		var replies [NUM_SERVERS]CommitReply
		for i := 0; i < NUM_SERVERS; i++ {
			go func(com *CommitArgs, reply *CommitReply, j int) {
				err := t.rpcClients[j].Call("Server.Commit", com, reply)
				if err != nil {
					c <- err
				} else {
					c <- nil
				}
			}(&com, &replies[i], i)
		}

		// Wait for responses
		var r error
		for i := 0; i < NUM_SERVERS; i++ {
			r = <-c
			if r != nil {
				log.Fatal("Error in commit: ", r)
			}
			log.Printf("Got commit %v/%v", i, NUM_SERVERS)
		}

		log.Printf("Done COMMIT %d", com.Uuid)
	}
}

func (t *Server) mergeWorker() {
	for {
		time.Sleep(MERGE_TIME_DELAY * time.Second)
		t.sendMergeRequest()
	}
}

func (t *Server) sendMergeRequest() {
	// Call each server and ask for their data
	// Send out COMMIT request
	c := make(chan error, NUM_SERVERS)
	var replies [NUM_SERVERS]DumpReply
	for i := 0; i < NUM_SERVERS; i++ {
		go func(reply *DumpReply, j int) {
			err := t.rpcClients[j].Call("Server.DumpTable", 0, reply)
			if err != nil {
				c <- err
			} else {
				c <- nil
			}
		}(&replies[i], i)
	}

	// Wait for responses
	var r error
	for i := 0; i < NUM_SERVERS; i++ {
		r = <-c
		if r != nil {
			log.Fatal("Error in merge: ", r)
		}
		log.Printf("Done merge")
	}

	var parg PlaintextArgs
	parg.Plaintext = revealCleartext(replies)

	c = make(chan error, NUM_SERVERS)
	for i := 0; i < NUM_SERVERS; i++ {
		go func(j int) {
			var p_reply PlaintextReply
			err := t.rpcClients[j].Call("Server.StorePlaintext", &parg, &p_reply)
			c <- err
		}(i)
	}

	// Wait for responses
	for i := 0; i < NUM_SERVERS; i++ {
		r = <-c
		if r != nil {
			log.Fatal("Error in plaintext: ", r)
		}
	}

	log.Printf("Done MERGE")
	MemCleanup()
}

func revealCleartext(tables [NUM_SERVERS]DumpReply) *BitMatrix {
	b := new(BitMatrix)

	// XOR all of the tables together and save
	// it in the plaintext table
	log.Printf("Revealing cleartext")
	for serv := 0; serv < NUM_SERVERS; serv++ {
		for i := 0; i < TABLE_HEIGHT; i++ {
			XorRows(&b[i], &tables[serv].Entries[i])
		}
	}
	log.Printf("Done revealing cleartext")

	return b
}

func (t *Server) beginMerge() {
	// Stop accepting uploads
	t.State = State_PrepareForMerge

	// Insert a nil request into the pipeline so that we can figure
	// out when all pending requests have been processed.
	incomingReqs <- beginMergeMarker
}

/**************
 * Handle Updates
 */

func (t *Server) Prepare(prep *PrepareArgs, reply *PrepareReply) error {
	var err error
	plainQueries := make([]*InsertQuery, len(prep.Queries))
	reply.QueryToAudit = make([]EncryptedAuditQuery, len(prep.Queries))

	n_queries := len(prep.Queries)
	c := make(chan int, n_queries)
	for i := 0; i < n_queries; i++ {
		go func(j int) {
			var err2 error
			plainQueries[j], err2 = DecryptQuery(t.ServerIdx, prep.Queries[j])
			if err2 != nil {
				panic("Decryption error")
			}
			c <- 0
		}(i)
	}

	for i := 0; i < n_queries; i++ {
		<-c
	}

	// Generate out each of the seeds and XOR into the database.
	// At the same time, generate an XOR of all of the seed outputs.
	log.Printf("XORing into table for %v", prep.Uuid)
	rows, err := t.entries.processQueries(plainQueries)
	log.Printf("Done XORing into table for %v", prep.Uuid)
	if err != nil {
		reply.Okay = false
		log.Println("Error in decrypt/audit: ", err)
		return err
	}

	for i := 0; i < n_queries; i++ {
		go func(j int) {
			log.Printf("Preparing audit for %v[%v]", prep.Uuid, j)
			// Use the XORd seeds (row) to generate the audit request
			reply.QueryToAudit[j] = prepareAudit(prep.Uuid, j, t.ServerIdx, plainQueries[j], &rows[j])
			log.Printf("Done preparing audit for %v[%v]", prep.Uuid, j)
			c <- 0
		}(i)
	}

	for i := 0; i < n_queries; i++ {
		<-c
	}

	reply.Okay = true
	t.pendingMutex.Lock()
	t.pending[prep.Uuid] = plainQueries
	t.pendingMutex.Unlock()

	return nil
}

func (t *Server) Commit(com *CommitArgs, reply *CommitReply) error {
	t.pendingMutex.Lock()
	queries, ok := t.pending[com.Uuid]
	t.pendingMutex.Unlock()

	if !ok {
		err := errors.New(fmt.Sprintf("Got commit msg for unknown UUID: %d", com.Uuid))
		return err
	}

	bogus := make([]*InsertQuery, 0)
	for i := range queries {
		if !com.Commit[i] {
			// Remove query from the database, since it
			// was malformed.
			log.Printf("Removing bogus query %v[%v] from DB", com.Uuid, i)
			bogus = append(bogus, queries[i])
		}
	}

	if len(bogus) > 0 {
		t.entries.processQueries(bogus)
	}

	t.pendingMutex.Lock()
	delete(t.pending, com.Uuid)
	t.pendingMutex.Unlock()

	t.clientsServedMutex.Lock()
	t.clientsServed = len(queries)
	elapsed := time.Now().Sub(t.clientsServedStart).Seconds()
	rate := float64(len(queries)) / time.Now().Sub(t.clientsServedStart).Seconds()
	log.Printf("Processed %v queries in %vs (rate %v reqs/sec) | table size %d", t.clientsServed, elapsed, rate,
		TABLE_WIDTH*TABLE_HEIGHT*SLOT_LENGTH)
	t.clientsServedStart = time.Now()
	t.clientsServedMutex.Unlock()

	return nil
}

func (t *Server) StorePlaintext(args *PlaintextArgs, reply *PlaintextReply) error {
	t.clientsServedMutex.Lock()
	t.clientsServed = 0
	t.clientsServedStart = time.Now()
	t.clientsServedMutex.Unlock()

	log.Printf("Storing plaintext")
	t.plainMutex.Lock()
	t.plain = args.Plaintext
	/*
	  var zeros SlotContents
	  for i := range t.plain {
	    for j := 0; j < len(t.plain[i]); j += SLOT_LENGTH {
	      msg := t.plain[i][j:(j+SLOT_LENGTH)]
	      if bytes.Compare(zeros[:], msg) != 0 {
	        log.Printf("Got msg: %v", msg)
	      }
	    }
	  }
	*/
	t.plainMutex.Unlock()

	t.State = State_AcceptUpload

	MemCleanup()
	return nil
}

func (t *Server) DumpTable(_ *int, reply *DumpReply) error {
	log.Printf("Dumping table %d\n", t.ServerIdx)
	reply.Entries = new(BitMatrix)
	t.entries.CopyToAndClear(reply.Entries)
	return nil
}

/*
func (t *Server) DumpPlaintext(_ *int, reply *DumpReply) error {
  t.plainMutex.Lock()
  reply.Entries = t.plain
  t.plainMutex.Unlock()
  return nil
}
*/

/***********
 * Initialization
 */

func (t *Server) connectToServer(client **rpc.Client, serverAddr string, remoteIdx int, c chan error) {
	var err error
	certs := []tls.Certificate{utils.ServerCertificates[remoteIdx]}
	*client, err = utils.DialHTTPWithTLS("tcp", serverAddr, t.ServerIdx, certs)
	c <- err
}

func (t *Server) openConnections() error {
	log.Printf("Waiting 2 seconds for other servers to boot")
	time.Sleep(1000 * time.Millisecond)

	if !t.isLeader() {
		return errors.New("Only leader should open connections")
	}

	c := make(chan error, len(t.ServerAddrs))
	for i := range t.ServerAddrs {
		go t.connectToServer(&t.rpcClients[i], t.ServerAddrs[i], i, c)
	}

	// Wait for all connections
	failed := false
	for i := 0; i < len(t.ServerAddrs); i++ {
		err := <-c
		if err != nil {
			log.Printf("Error connecting to server: %v", err)
		}
	}

	if failed {
		return errors.New("Connection failed")
	} else {
		t.State = State_AcceptUpload
		return nil
	}
}

func (t *Server) Initialize(*int, *int) error {
	if t.isLeader() {
		go t.submitPrepares()
		go t.submitAudits()
		go t.submitCommits()
		go t.mergeWorker()
		go func(t *Server) {
			// HACK wait until other servers have started
			time.Sleep(500 * time.Millisecond)
			err := t.openConnections()
			if err != nil {
				log.Fatal("Could not initialize table", err)
			}
		}(t)
	}
	return nil
}

func elementsToBytes(elms []group.Element) []byte {
	var buf bytes.Buffer
	for i := 0; i < len(elms); i++ {
		buf.Write(utils.CommonCurve.Marshal(elms[i]))
	}

	return buf.Bytes()
}

/*
func (t *Server) Download(args *DownloadArgs, reply *DownloadReply) error {
  log.Printf("Got download request")
  log.Printf("Request:", args)

  if !RangeIsValid(t, args.RequestedSlot) {
    return errors.New("Out of range")
  }

  var slot = &t.Entries[args.RequestedSlot]
  log.Printf("idx: ", args.RequestedSlot)

  slot.Mutex.Lock()
  if slot.IsFilled {
    reply.Data.Buffer = slot.Data.Buffer
  }
  slot.Mutex.Unlock()

  return nil
}
*/

func NewServer(serverIdx int, serverAddrs []string) *Server {
	t := new(Server)
	t.entries = new(SlotTable)
	t.plain = new(BitMatrix)
	t.ServerIdx = serverIdx
	t.ServerAddrs = serverAddrs
	t.clientsServed = 0
	t.clientsServedStart = time.Now()
	t.pending = map[int64]([]*InsertQuery){}

	return t
}

func (t *Server) DoNothing(args *int, reply *int) error {
	// Just use this to test number
	// of requests can handle in a second
	t.clientsServedMutex.Lock()
	t.clientsServed++
	log.Printf("Served %v", t.clientsServed)
	t.clientsServedMutex.Unlock()

	return nil
}

func init() {
	beginMergeMarkerCommit.Uuid = 0
}
