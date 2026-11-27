pipeline {
    agent any
    stages {
        stage("Build") {
            steps {
                script {
                    env.() = "v"
                }
            }
        }
    }
}
