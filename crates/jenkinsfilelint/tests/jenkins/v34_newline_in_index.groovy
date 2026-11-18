pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def m = [a: 1]
                    echo m['a'
                        ].toString()
                }
            }
        }
    }
}
