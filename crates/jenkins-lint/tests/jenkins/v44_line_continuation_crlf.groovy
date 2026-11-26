pipeline {
    agent any
    stages {
        stage("A") {
            steps {
                script {
                    def x = 1 + \
                        2
                }
            }
        }
    }
}
