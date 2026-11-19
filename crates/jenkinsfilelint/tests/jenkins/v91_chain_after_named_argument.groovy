pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script { sh script: "find -name "${n}" | egrep ." }
            }
        }
    }
}
