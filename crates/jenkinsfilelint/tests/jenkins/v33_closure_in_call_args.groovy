pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def r = [1, 2].collect({ it ->
                        it * 2
                    })
                }
            }
        }
    }
}
