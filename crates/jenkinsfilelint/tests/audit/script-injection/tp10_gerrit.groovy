node {
    sh "echo ${GERRIT_CHANGE_SUBJECT}"
    sh "notify --topic ${env.GERRIT_TOPIC} --owner ${env.GERRIT_CHANGE_OWNER_NAME}"
    sh "echo ${GERRIT_CHANGE_COMMIT_MESSAGE} | base64 -d"
    sh "git fetch origin ${GERRIT_BRANCH}"
}
