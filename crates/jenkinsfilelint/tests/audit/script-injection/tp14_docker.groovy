node {
    def image = docker.build('app', "--build-arg BRANCH=${env.CHANGE_BRANCH} .")
    docker.image('postgres:16').withRun("-e POSTGRES_DB=${params.DB}") { c ->
        sh 'make test'
    }
    def tool = docker.image('tool:1.0')
    tool.run("--name ${env.CHANGE_BRANCH}", "echo ${env.CHANGE_TITLE}")
    docker.build("app:${env.CHANGE_BRANCH}")
}
