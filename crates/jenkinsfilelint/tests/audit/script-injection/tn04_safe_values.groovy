node {
    sh "make -j${params.JOBS.toInteger()}"
    sh "make -j${params.JOBS as Integer}"
    sh "echo ${env.CHANGE_TITLE.size()}"
    sh "echo ${env.CHANGE_TITLE.contains('WIP') ? 'draft' : 'ready'}"
    sh "echo ${env.CHANGE_BRANCH == 'main'}"
    sh "curl https://example.com/?q=${URLEncoder.encode(env.CHANGE_TITLE, 'UTF-8')}"
    sh "docker tag app app:${env.CHANGE_BRANCH.replaceAll(/[^A-Za-z0-9._-]/, '-')}"
    sh "echo '${env.CHANGE_TITLE.replace("'", "'\\''")}'"
    sh "echo ${env.CHANGE_TITLE.bytes.encodeBase64()}"
}
