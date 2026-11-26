node {
    def subject = sh(script: 'git log -1 --format=%s', returnStdout: true).trim()
    def pr = github.lookup(env.CHANGE_TITLE, env.CHANGE_BRANCH)
    sh "echo ${subject} ${pr}"
}
